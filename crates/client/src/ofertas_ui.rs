//! FLASH OFFERS panel (docs/OFERTAS_RELAMPAGO.md).
//!
//! Always reachable — Menu › Event › Offers and its own icon in the quick bar
//! at the top right — with a red badge while an offer is open (owner,
//! 08/11/2026). A NEW offer pops the panel open by itself.
//!
//! Only a client that can pay asks the server for offers (Android, Google
//! Play): on PC/iPhone the hour would run out on an offer nobody can buy.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::loja::{OfertaRelampago, PedidoLoja};
use shared::protocol::ClientMessage;

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as estilo;

/// Real payments exist here (Google Play Billing).
pub const PODE_PAGAR: bool = cfg!(target_os = "android");

#[derive(Default)]
pub struct OfertasUi {
    pub aberto: bool,
    /// Open offers and the `get_time()` they close at.
    ofertas: Vec<(OfertaRelampago, f64)>,
    /// Offer whose Google Play sheet is open now.
    pendente: Option<String>,
    em_voo: bool,
    ultimo: Option<(bool, String)>,
    /// Level of the last check (a new level may unlock an offer).
    nivel_checado: Option<u32>,
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

impl OfertasUi {
    fn pedido() -> ClientMessage {
        ClientMessage::Loja { pedido: PedidoLoja::OfertasRelampago }
    }

    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        if PODE_PAGAR { vec![Self::pedido()] } else { Vec::new() }
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// Any offer still running: the red badge on the icon and on MENU.
    pub fn ativa(&self) -> bool {
        let agora = get_time();
        self.ofertas.iter().any(|(_, fim)| *fim > agora)
    }

    /// Once per frame in the world: asks again when the level changed (the
    /// first frame counts as a change).
    pub fn checar(&mut self, nivel: u32) -> Option<ClientMessage> {
        if !PODE_PAGAR || nivel == 0 || self.nivel_checado == Some(nivel) {
            return None;
        }
        self.nivel_checado = Some(nivel);
        Some(Self::pedido())
    }

    pub fn receber(&mut self, v: Vec<OfertaRelampago>) {
        let agora = get_time();
        if v.iter().any(|o| o.nova) {
            self.aberto = true;
            crate::sons::tocar(crate::sons::Som::Clique);
        }
        self.ofertas = v
            .into_iter()
            .map(|o| {
                let fim = agora + o.segundos as f64;
                (o, fim)
            })
            .collect();
    }

    /// The shop's answer while a purchase of ours was in flight.
    pub fn resultado(&mut self, ok: bool, texto: &str) {
        if self.em_voo {
            self.em_voo = false;
            self.ultimo = Some((ok, texto.to_string()));
        }
    }

    /// Google Play closed without a purchase (cancelled, error).
    pub fn cancelou(&mut self) {
        if self.pendente.take().is_some() {
            self.em_voo = false;
            self.ultimo = Some((false, "Purchase canceled".into()));
        }
    }

    /// Which offer a paid Play product belongs to: the one whose sheet we
    /// opened, else any open offer on that product (a purchase recovered at
    /// login).
    pub fn oferta_do_produto(&mut self, produto: &str) -> Option<String> {
        if let Some(id) = self.pendente.take() {
            if self.ofertas.iter().any(|(o, _)| o.id == id && o.produto_play == produto) {
                return Some(id);
            }
        }
        self.ofertas
            .iter()
            .find(|(o, _)| o.produto_play == produto)
            .map(|(o, _)| o.id.clone())
    }

    pub fn desenha(&mut self, nomes: &HashMap<u16, String>) -> Vec<ClientMessage> {
        if !self.aberto {
            return Vec::new();
        }
        estilo::no_painel(estilo::escala_do_painel(760.0, 560.0), || self.desenha_na_escala(nomes))
    }

    fn desenha_na_escala(&mut self, nomes: &HashMap<u16, String>) -> Vec<ClientMessage> {
        let saida = Vec::new();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (760.0 * f).min(seguro.w - 16.0);
        let h = (560.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Flash offers", 20, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 7.0 * f, "X", 18, estilo::TEXTO);
        if clicou && fechar.toca(m) {
            self.fechar();
            return saida;
        }
        let agora = get_time();
        self.ofertas.retain(|(_, fim)| *fim > agora - 1.0);
        // THE "RATE US" PROMOTION, always on top (owner, 10/11/2026: rate on
        // Google Play, post the screenshot on Discord, get a gift in-game).
        let promo = Rect::new(x0, p.y + 54.0 * f, p.w - 40.0 * f, 196.0 * f);
        desenha_promo_avaliacao(promo, f, m, clicou);
        let topo = promo.y + promo.h + 12.0 * f;
        if !PODE_PAGAR {
            estilo::texto(x0, topo + 24.0 * f, "Flash offers are available in the Android app (Google Play).", 15, estilo::SUAVE);
            return saida;
        }
        if self.ofertas.is_empty() {
            estilo::texto(x0, topo + 24.0 * f, "No flash offer right now.", 16, estilo::TEXTO);
            estilo::texto(x0, topo + 50.0 * f, "Keep leveling up: new offers unlock at higher levels.", 14, estilo::SUAVE);
            return saida;
        }
        let pt = shared::idioma::atual() == shared::idioma::Idioma::Pt;
        let mut y = topo;
        let alto = ((p.y + p.h - 50.0 * f - y) / self.ofertas.len() as f32).min(400.0 * f);
        let mut comprar: Option<(String, String)> = None;
        for (o, fim) in &self.ofertas {
            let r = Rect::new(x0, y, p.w - 40.0 * f, alto - 10.0 * f);
            estilo::cartao(r, false, true);
            let titulo = if pt { o.titulo_pt.clone().unwrap_or_else(|| o.titulo.clone()) } else { o.titulo.clone() };
            estilo::texto_forte(r.x + 16.0 * f, r.y + 30.0 * f, &titulo, 18, estilo::OURO);
            let resta = fim - agora;
            let t = relogio(resta);
            estilo::texto_forte(
                r.x + r.w - 16.0 * f - estilo::medir_forte(&t, 18),
                r.y + 30.0 * f,
                &t,
                18,
                if resta < 300.0 { estilo::VERMELHO } else { estilo::TEXTO },
            );
            // Contents: TP, energy, then each item with its icon.
            let mut linhas: Vec<(Option<u16>, String)> = Vec::new();
            if o.tp > 0 {
                linhas.push((None, format!("{} TP", shared::idioma::milhar(o.tp as u64))));
            }
            if o.energia > 0 {
                linhas.push((None, format!("{} Energy", shared::idioma::milhar(o.energia))));
            }
            for (id, q) in &o.itens {
                let nome = nomes.get(id).cloned().unwrap_or_else(|| format!("#{id}"));
                linhas.push((Some(*id), format!("{}× {nome}", shared::idioma::milhar(*q as u64))));
            }
            let lado = 30.0 * f;
            for (i, (id, texto)) in linhas.iter().enumerate() {
                let col = (i % 2) as f32;
                let lin = (i / 2) as f32;
                let lx = r.x + 16.0 * f + col * (r.w - 32.0 * f) * 0.5;
                let ly = r.y + 48.0 * f + lin * (lado + 6.0 * f);
                if let Some(id) = id {
                    crate::icones::icone(*id, Rect::new(lx, ly, lado, lado), None, None);
                }
                estilo::texto(lx + lado + 8.0 * f, ly + lado * 0.7, texto, 14, estilo::TEXTO);
            }
            // Price + buy.
            let b = Rect::new(r.x + r.w - 236.0 * f, r.y + r.h - 56.0 * f, 220.0 * f, 44.0 * f);
            if let Some(v) = o.valor_de_centavos.filter(|v| *v > o.preco_centavos) {
                let de = preco(v);
                let dx = b.x - 16.0 * f - estilo::medir(&de, 14);
                estilo::texto(dx, b.center().y + 5.0 * f, &de, 14, estilo::SUAVE);
                let wl = estilo::medir(&de, 14);
                draw_line(dx, b.center().y, dx + wl, b.center().y, 1.5, estilo::SUAVE);
            }
            let livre = !self.em_voo && resta > 0.0;
            let sobre = livre && b.toca(m);
            estilo::botao(
                b,
                &format!("{}  {}", shared::idioma::tr("Buy"), preco(o.preco_centavos)),
                estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), !livre, false),
                true,
            );
            if sobre && clicou {
                comprar = Some((o.id.clone(), o.produto_play.clone()));
            }
            y += alto;
        }
        if let Some((id, produto)) = comprar {
            self.em_voo = true;
            self.ultimo = None;
            self.pendente = Some(id);
            #[cfg(target_os = "android")]
            crate::billing_android::launch_product(&produto);
            #[cfg(not(target_os = "android"))]
            let _ = produto;
        }
        if self.em_voo {
            estilo::texto(x0, p.y + p.h - 20.0 * f, "Waiting for Google Play…", 14, estilo::SUAVE);
        } else if let Some((ok, t)) = &self.ultimo {
            estilo::texto(x0, p.y + p.h - 20.0 * f, t, 14, if *ok { estilo::VERDE } else { estilo::VERMELHO });
        }
        saida
    }
}

const DISCORD: &str = "https://discord.gg/WpbF3EcVg";

/// The "Rate us & get a gift" card: three steps, the gift with its icons,
/// and the two buttons (Google Play, Discord). The gift is sent by the staff
/// by mail once the screenshot is on Discord.
fn desenha_promo_avaliacao(r: Rect, f: f32, m: Vec2, clicou: bool) {
    estilo::painel_destaque(r, estilo::VERDE);
    let x = r.x + 16.0 * f;
    estilo::texto_forte(x, r.y + 30.0 * f, "Rate us & get a gift!", 19, estilo::VERDE);
    let passos = [
        "1. Rate Tempest on Google Play.",
        "2. Post a screenshot of your review on our Discord.",
        "3. We send you a gift to your in-game Mail:",
    ];
    for (i, t) in passos.iter().enumerate() {
        estilo::texto(x, r.y + (56.0 + i as f32 * 21.0) * f, t, 14, estilo::TEXTO);
    }
    // The gift: 5,000 Darksteel · 5,000 Energy · 1 Mount Summon Scroll.
    let lado = 28.0 * f;
    let gy = r.y + 108.0 * f;
    let mut gx = x + 8.0 * f;
    let presente: [(Option<u16>, &str); 3] = [
        (Some(shared::item_id::DARKSTEEL), "5,000 Darksteel"),
        (None, "5,000 Energy"),
        (Some(shared::item_id::PERGAMINHO_INVOCA_MONTARIA), "1 Mount Summon Scroll"),
    ];
    for (id, t) in presente {
        match id {
            Some(id) if id == shared::item_id::PERGAMINHO_INVOCA_MONTARIA => {
                crate::invocacao_ui::icone_pergaminho_de(id, vec2(gx + lado * 0.5, gy + lado * 0.5), lado)
            }
            Some(id) => crate::icones::icone(id, Rect::new(gx, gy, lado, lado), None, None),
            None => estilo::icone_energia(vec2(gx + lado * 0.5, gy + lado * 0.5), lado * 0.8),
        }
        estilo::texto_forte(gx + lado + 6.0 * f, gy + lado * 0.7, t, 14, Color::new(0.99, 0.86, 0.45, 1.0));
        gx += lado + 14.0 * f + estilo::medir_forte(t, 14);
    }
    let bh = 40.0 * f;
    let by = r.y + r.h - bh - 10.0 * f;
    let play = Rect::new(r.x + r.w - 2.0 * (190.0 * f) - 26.0 * f, by, 190.0 * f, bh);
    let discord = Rect::new(r.x + r.w - 190.0 * f - 16.0 * f, by, 190.0 * f, bh);
    estilo::botao(play, "Rate on Google Play", estilo::estado_de(play, false, false), true);
    estilo::botao(discord, "Open Discord", estilo::estado_de(discord, false, false), false);
    if clicou && play.toca(m) {
        crate::avaliacao::abrir_loja();
    } else if clicou && discord.toca(m) {
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
}

/// `MMO_PREVIA_OFERTAS=1`: the Offers window with the promotion card.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-ofertas-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    let mut ui = OfertasUi { aberto: true, ..Default::default() };
    for _ in 0..3 {
        set_camera(&Camera2D {
            render_target: Some(rt.clone()),
            ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()))
        });
        clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
        let _ = ui.desenha(&HashMap::new());
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/ofertas.png"));
        set_default_camera();
        next_frame().await;
    }
}
