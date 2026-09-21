//! Loja de cash (docs/LOJA.md): Menu → Comércio → Loja. A vitrine mais
//! caprichada do jogo: montaria em 3D girando num palco (arrastar gira),
//! skins trocadas no preview, pacotes de TP com arte e o saldo animado.
//!
//! Tudo que vale e' do servidor: a janela so' mostra o catalogo
//! (`shared::loja`) e pede. Cada compra leva um id de pedido novo, gerado
//! aqui: clique duplo e reenvio valem uma compra so' no banco.
//!
//! O modelo 3D e' desenhado direto na tela num viewport
//! (`render3d::vitrine_montaria`), que e' o que o iPhone aceita. No maximo
//! quatro montarias por quadro: o palco e as tres miniaturas.
use std::f32::consts::TAU;

use macroquad::prelude::*;
use shared::loja::{self as cat, AvisoLoja, EstadoLoja, PedidoLoja, Produto};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

/// (rotulo, icone de HUD; vazio = o cristal do TP).
/// A aba dos pacotes de TP. Nomeada porque varios caminhos ("+" do saldo,
/// saldo insuficiente) mandam pra ela — com indice na mao, inserir uma aba no
/// meio levava o jogador pro lugar errado.
const ABA_TP: usize = 3;

const ABAS: [(&str, &str); 4] = [
    ("Materiais", "craft"),
    ("Pets", "montaria"),
    ("Moedas", "bolsa"),
    ("Tempest Points", ""),
];

// Paleta premium, so' da loja: roxo profundo com dourado.
const ROXO_TOPO: Color = Color::new(0.115, 0.066, 0.215, 0.985);
const NOITE: Color = Color::new(0.028, 0.030, 0.075, 0.985);
const OURO_CLARO: Color = Color::new(1.0, 0.90, 0.58, 1.0);
const OURO_ESCURO: Color = Color::new(0.83, 0.54, 0.15, 1.0);
const LILAS: Color = Color::new(0.72, 0.62, 1.0, 1.0);
const CIANO: Color = Color::new(0.55, 0.86, 1.0, 1.0);
const TINTA_BOTAO: Color = Color::new(0.17, 0.09, 0.02, 1.0);
const VERDE_POSSE: Color = Color::new(0.46, 0.92, 0.66, 1.0);
const AMBAR: Color = Color::new(1.0, 0.72, 0.30, 1.0);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Confirma {
    Tp(u16),
    Item(Produto),
}

/// O que o botao de um item mostra.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Situacao {
    Comprar(u64),
    Possui,
    /// Skin padrao: vem com a montaria.
    Inclusa,
    /// Skin a venda de montaria que a conta ainda nao tem.
    RequerMontaria,
}

#[derive(Default)]
pub struct LojaTp {
    pub aberto: bool,
    estado: Option<EstadoLoja>,
    aba: usize,
    confirma: Option<Confirma>,
    /// Quantas unidades do lote na confirmacao. Volta a 1 a cada abertura —
    /// lote herdado da compra anterior e' compra sem querer. Lido sempre por
    /// `cat::lote`, que clampa: o `Default` do struct da' 0.
    lote: u16,
    /// Ultimo resultado (ok, texto), no rodape.
    ultimo: Option<(bool, String)>,
    /// Compra enviada e sem resposta: o botao fica "Aguarde".
    em_voo: bool,
    contador: u32,
    /// Giro do modelo no palco (radianos) e o x do dedo arrastando.
    giro: f32,
    arrasto: Option<f32>,
    /// Comemoracao de compra: texto esperando o primeiro quadro, depois
    /// (inicio, texto).
    festa_pendente: Option<String>,
    festa: Option<(f64, String)>,
    /// Saldo desenhado: corre ate' o do servidor.
    tp_mostrado: f64,
}




/// Selo de vitrine da montaria.
pub fn selo_da_montaria(id: u16) -> Option<&'static str> {
    match id {
        2 => Some("POPULAR"),
        3 => Some("NOVO"),
        _ => None,
    }
}

/// Selo de vitrine do pacote de TP.
pub fn selo_do_pacote(id: u16) -> Option<&'static str> {
    match id {
        2 => Some("POPULAR"),
        4 => Some("MELHOR VALOR"),
        _ => None,
    }
}

/// Bonus do pacote em % sobre a TP base (arredonda pra baixo).
pub fn bonus_pct(pk: &cat::PacoteTp) -> u64 {
    if pk.tp == 0 {
        0
    } else {
        pk.bonus * 100 / pk.tp
    }
}

/// Saldo depois de pagar `preco`; `None` = nao da'.
pub fn saldo_apos(tp: u64, preco: u64) -> Option<u64> {
    tp.checked_sub(preco)
}

/// O saldo desenhado anda ate' o alvo (contagem animada), e cola no fim.
fn anima_saldo(atual: f64, alvo: f64, dt: f32) -> f64 {
    let novo = atual + (alvo - atual) * (dt as f64 * 5.0).min(1.0);
    if (alvo - novo).abs() < 0.5 {
        alvo
    } else {
        novo
    }
}

fn milhar(v: u64) -> String {
    crate::economia::milhar(v)
}

fn fracao(x: f32) -> f32 {
    x - x.floor()
}

/// Pseudo-aleatorio estavel por indice (particulas sem crate de rand).
fn sorteio(i: u32) -> f32 {
    fracao((i as f32 * 12.9898 + 4.1414).sin() * 43_758.547)
}

/// Tamanho de texto que acompanha o layout (`k`) e nao so' a escala da UI.
fn ts(n: f32, k: f32) -> u16 {
    (n * k / estilo::fator_texto()).round().clamp(7.0, 220.0) as u16
}

/// Brilho redondo e suave: aneis concentricos translucidos.
fn brilho_radial(c: Vec2, r: f32, cor: Color, forca: f32) {
    const N: usize = 12;
    for i in 0..N {
        let rr = r * (1.0 - i as f32 / N as f32);
        draw_circle(c.x, c.y, rr, estilo::alfa(cor, forca / N as f32 * 1.6));
    }
}

/// Faiscas subindo devagar dentro de `r`.
fn faiscas(r: Rect, agora: f64, n: u32, k: f32, semente: u32) {
    let t = agora as f32;
    for i in 0..n {
        let a = sorteio(i * 7 + semente);
        let b = sorteio(i * 13 + semente + 1);
        let vel = 0.035 + 0.06 * sorteio(i * 3 + semente + 2);
        let subida = fracao(b + t * vel);
        let x = r.x + r.w * (0.06 + 0.88 * a) + (t * 0.7 + i as f32).sin() * 6.0 * k;
        let y = r.y + r.h * (1.0 - subida);
        let alfa = (subida * std::f32::consts::PI).sin() * (0.35 + 0.45 * sorteio(i + semente + 9));
        let cor = if i % 3 == 0 {
            OURO_CLARO
        } else if i % 3 == 1 {
            CIANO
        } else {
            LILAS
        };
        let raio = (1.2 + 1.8 * sorteio(i * 5 + semente)) * k.max(0.6);
        draw_circle(x, y, raio * 2.4, estilo::alfa(cor, alfa * 0.18));
        draw_circle(x, y, raio, estilo::alfa(cor, alfa));
    }
}

/// Pilula de selo ("POPULAR"). `canto` = topo-esquerdo. Devolve a largura.
fn selo(canto: Vec2, texto: &str, k: f32, forte: bool) -> f32 {
    let tam = ts(11.0, k);
    let w = estilo::medir_forte(texto, tam) + 20.0 * k;
    let r = Rect::new(canto.x, canto.y, w, 22.0 * k);
    let (a, b) = if forte {
        (OURO_CLARO, OURO_ESCURO)
    } else {
        (
            Color::new(0.62, 0.52, 1.0, 1.0),
            Color::new(0.38, 0.26, 0.86, 1.0),
        )
    };
    estilo::ret_gradiente(r, r.h * 0.5, a, b);
    estilo::borda_arredondada(r, r.h * 0.5, 1.0, estilo::alfa(WHITE, 0.35));
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + 11.0 * k * 0.36,
        texto,
        tam,
        if forte { TINTA_BOTAO } else { WHITE },
    );
    w
}

/// Botao dourado grande. So' desenha; quem chamou decide o clique.
fn botao_ouro(r: Rect, rotulo: &str, ativo: bool, sobre: bool, k: f32, agora: f64) {
    estilo::sombra(r, r.h * 0.5, 0.8);
    if ativo {
        let (a, b) = if sobre {
            (
                estilo::clarear(OURO_CLARO, 0.08),
                estilo::clarear(OURO_ESCURO, 0.12),
            )
        } else {
            (OURO_CLARO, OURO_ESCURO)
        };
        // halo que respira em volta: chama o olho sem piscar
        let pulso = 0.5 + 0.5 * (agora as f32 * 2.4).sin();
        estilo::borda_arredondada(
            Rect::new(r.x - 3.0 * k, r.y - 3.0 * k, r.w + 6.0 * k, r.h + 6.0 * k),
            r.h * 0.5 + 3.0 * k,
            2.0 * k.max(0.7),
            estilo::alfa(OURO_CLARO, 0.12 + 0.22 * pulso),
        );
        estilo::ret_gradiente(r, r.h * 0.5, a, b);
        estilo::ret_arredondado(
            Rect::new(r.x + r.h * 0.25, r.y + 3.0 * k, r.w - r.h * 0.5, r.h * 0.40),
            r.h * 0.2,
            estilo::alfa(WHITE, 0.22),
        );
        estilo::borda_arredondada(
            r,
            r.h * 0.5,
            1.5 * k.max(0.7),
            Color::new(0.55, 0.32, 0.05, 0.9),
        );
    } else {
        estilo::ret_gradiente(
            r,
            r.h * 0.5,
            Color::new(0.30, 0.28, 0.36, 1.0),
            Color::new(0.18, 0.17, 0.23, 1.0),
        );
        estilo::borda_arredondada(r, r.h * 0.5, 1.0, estilo::alfa(WHITE, 0.15));
    }
    let tam = ts(17.0, k);
    let px = 17.0 * k;
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + px * 0.36,
        rotulo,
        tam,
        if ativo { TINTA_BOTAO } else { estilo::SUAVE },
    );
}

/// Botao de contorno (Cancelar).
fn botao_contorno(r: Rect, rotulo: &str, sobre: bool, k: f32) {
    estilo::ret_arredondado(
        r,
        r.h * 0.5,
        estilo::alfa(WHITE, if sobre { 0.10 } else { 0.05 }),
    );
    estilo::borda_arredondada(
        r,
        r.h * 0.5,
        1.5 * k.max(0.7),
        estilo::alfa(LILAS, if sobre { 0.7 } else { 0.4 }),
    );
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + 16.0 * k * 0.36,
        rotulo,
        ts(16.0, k),
        estilo::TEXTO,
    );
}

/// Estado sem compra: "POSSUÍDA", "INCLUSA", "REQUER A MONTARIA".
fn etiqueta_estado(r: Rect, rotulo: &str, cor: Color, k: f32) {
    estilo::ret_arredondado(r, r.h * 0.5, estilo::alfa(cor, 0.12));
    estilo::borda_arredondada(r, r.h * 0.5, 1.5 * k.max(0.7), estilo::alfa(cor, 0.6));
    estilo::texto_centro_forte(
        r.center().x,
        r.center().y + 15.0 * k * 0.36,
        rotulo,
        ts(15.0, k),
        cor,
    );
}

/// Pilula com icone de HUD e texto. Devolve a largura.
fn chip(x: f32, y: f32, icone: &str, texto: &str, k: f32) -> f32 {
    let tam = ts(13.0, k);
    let w = estilo::medir(texto, tam) + if icone.is_empty() { 20.0 } else { 44.0 } * k;
    let r = Rect::new(x, y, w, 28.0 * k);
    estilo::ret_arredondado(r, r.h * 0.5, Color::new(0.0, 0.0, 0.0, 0.32));
    estilo::borda_arredondada(r, r.h * 0.5, 1.0, estilo::alfa(LILAS, 0.35));
    let mut tx = r.x + 10.0 * k;
    if !icone.is_empty() {
        crate::icones_ui::ui(
            icone,
            vec2(r.x + 18.0 * k, r.center().y),
            20.0 * k,
            OURO_CLARO,
        );
        tx = r.x + 32.0 * k;
    }
    estilo::texto(
        tx,
        r.center().y + 13.0 * k * 0.36,
        texto,
        tam,
        estilo::TEXTO,
    );
    w
}

/// Visto desenhado (nao depende de glifo na fonte).
fn visto(c: Vec2, s: f32, cor: Color) {
    estilo::traco(
        c + vec2(-s * 0.45, 0.0),
        c + vec2(-s * 0.1, s * 0.35),
        (s * 0.18).max(1.5),
        cor,
    );
    estilo::traco(
        c + vec2(-s * 0.1, s * 0.35),
        c + vec2(s * 0.5, -s * 0.35),
        (s * 0.18).max(1.5),
        cor,
    );
}

impl LojaTp {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.confirma = None;
        vec![ClientMessage::Loja {
            pedido: PedidoLoja::Estado,
        }]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.confirma = None;
        self.arrasto = None;
    }

    pub fn estado(&self) -> Option<&EstadoLoja> {
        self.estado.as_ref()
    }

    /// Chegou aviso do servidor. Devolve texto pro chat.
    pub fn receber(&mut self, aviso: AvisoLoja) -> Option<String> {
        match aviso {
            AvisoLoja::Estado(e) => {
                if self.estado.is_none() {
                    self.tp_mostrado = e.tp as f64;
                }
                self.estado = Some(e);
                self.em_voo = false;
                None
            }
            AvisoLoja::Resultado { ok, texto } => {
                self.em_voo = false;
                if ok {
                    self.festa_pendente = Some(texto.clone());
                }
                self.ultimo = Some((ok, texto.clone()));
                Some(format!("Loja: {texto}"))
            }
            AvisoLoja::Montando { .. } => None,
            AvisoLoja::Invocacao { .. } => None,
            AvisoLoja::Invocacoes { .. } => None,
        }
    }

    /// Id de pedido novo: relogio em nanossegundos + contador. Cabe nas regras
    /// de `shared::loja::pedido_valido`.
    fn novo_pedido(&mut self) -> String {
        self.contador = self.contador.wrapping_add(1);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let id = format!("c{ns:x}-{:x}", self.contador);
        debug_assert!(cat::pedido_valido(&id));
        id
    }


    pub fn desenha(&mut self, vox: &VoxCache, solido: &Material) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        let agora = get_time();
        let dt = get_frame_time().min(0.1);
        if let Some(t) = self.festa_pendente.take() {
            self.festa = Some((agora, t));
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        // A vitrine ocupa quase a tela toda (no iPhone sobra largura).
        let w = (1500.0 * f).min(seguro.w - 24.0);
        let h = (920.0 * f).min(seguro.h - 20.0);
        let k = (w / 1200.0).min(h / 740.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        let m = Vec2::from(mouse_position());
        let modal = self.confirma.is_some();
        let clicou = crate::foco::clique();
        let livre = clicou && !modal;
        let raio = 20.0 * k;

        // ── fundo ──
        crate::hud_layout::escurece(0.62);
        estilo::sombra(p, raio, 1.2);
        estilo::ret_gradiente(p, raio, ROXO_TOPO, NOITE);
        brilho_radial(
            vec2(p.x + p.w * 0.30, p.y + p.h * 0.58),
            p.h * 0.58,
            Color::new(0.45, 0.30, 0.95, 1.0),
            0.16,
        );
        brilho_radial(
            vec2(p.x + p.w * 0.88, p.y + p.h * 0.02),
            p.h * 0.42,
            Color::new(0.95, 0.66, 0.28, 1.0),
            0.10,
        );
        faiscas(p, agora, 26, k, 11);
        estilo::borda_arredondada(p, raio, 1.6 * k.max(0.8), estilo::alfa(OURO_CLARO, 0.38));
        draw_rectangle(
            p.x + 60.0 * k,
            p.y,
            p.w - 120.0 * k,
            2.0 * k.max(0.8),
            estilo::alfa(OURO_CLARO, 0.55),
        );

        // ── cabecalho ──
        let xh = p.x + 28.0 * k;
        estilo::icone_tp(vec2(xh + 26.0 * k, p.y + 48.0 * k), 64.0 * k);
        estilo::texto_sombra(
            xh + 64.0 * k,
            p.y + 50.0 * k,
            "LOJA",
            ts(30.0, k),
            OURO_CLARO,
            true,
        );
        estilo::texto(
            xh + 66.0 * k,
            p.y + 72.0 * k,
            "Itens exclusivos da Tempestade",
            ts(13.0, k),
            estilo::alfa(LILAS, 0.9),
        );

        let fechar_c = vec2(p.x + p.w - 36.0 * k, p.y + 42.0 * k);
        let fechar_r = 19.0 * k;
        let sobre_fechar = !modal && m.distance(fechar_c) <= fechar_r * 1.2;
        draw_circle(
            fechar_c.x,
            fechar_c.y,
            fechar_r,
            Color::new(0.0, 0.0, 0.0, if sobre_fechar { 0.55 } else { 0.35 }),
        );
        draw_circle_lines(
            fechar_c.x,
            fechar_c.y,
            fechar_r,
            1.5 * k.max(0.8),
            estilo::alfa(LILAS, if sobre_fechar { 0.8 } else { 0.45 }),
        );
        let s = fechar_r * 0.38;
        estilo::traco(
            fechar_c + vec2(-s, -s),
            fechar_c + vec2(s, s),
            2.2 * k.max(0.8),
            estilo::TEXTO,
        );
        estilo::traco(
            fechar_c + vec2(-s, s),
            fechar_c + vec2(s, -s),
            2.2 * k.max(0.8),
            estilo::TEXTO,
        );
        if livre && sobre_fechar {
            self.fechar();
            return saida;
        }

        let Some(estado) = self.estado.clone() else {
            estilo::texto_centro(
                p.center().x,
                p.center().y,
                "Carregando a vitrine…",
                ts(17.0, k),
                estilo::SUAVE,
            );
            return saida;
        };
        self.tp_mostrado = anima_saldo(self.tp_mostrado, estado.tp as f64, dt);

        // Saldo com o botao "+" (leva aos pacotes).
        let txt_saldo = milhar(self.tp_mostrado.round().max(0.0) as u64);
        let tam_saldo = ts(21.0, k);
        let cw = estilo::largura_tp_texto(&txt_saldo, tam_saldo, true) + 70.0 * k;
        let capsula = Rect::new(
            fechar_c.x - fechar_r - 18.0 * k - cw,
            p.y + 21.0 * k,
            cw,
            44.0 * k,
        );
        estilo::ret_gradiente(
            capsula,
            capsula.h * 0.5,
            Color::new(0.20, 0.13, 0.36, 1.0),
            Color::new(0.08, 0.06, 0.16, 1.0),
        );
        estilo::borda_arredondada(
            capsula,
            capsula.h * 0.5,
            1.5 * k.max(0.8),
            estilo::alfa(OURO_CLARO, 0.65),
        );
        estilo::tp_texto(
            capsula.x + 12.0 * k,
            capsula.center().y + 21.0 * k * 0.36,
            &txt_saldo,
            tam_saldo,
            estilo::TEXTO,
            true,
        );
        let mais_c = vec2(capsula.x + capsula.w - 22.0 * k, capsula.center().y);
        let sobre_mais = !modal && m.distance(mais_c) <= 17.0 * k;
        draw_circle(
            mais_c.x,
            mais_c.y,
            15.0 * k,
            if sobre_mais {
                estilo::clarear(OURO_CLARO, 0.1)
            } else {
                OURO_CLARO
            },
        );
        draw_circle(
            mais_c.x,
            mais_c.y - 3.0 * k,
            10.0 * k,
            estilo::alfa(WHITE, 0.25),
        );
        estilo::traco(
            mais_c + vec2(-7.0 * k, 0.0),
            mais_c + vec2(7.0 * k, 0.0),
            3.0 * k.max(0.7),
            TINTA_BOTAO,
        );
        estilo::traco(
            mais_c + vec2(0.0, -7.0 * k),
            mais_c + vec2(0.0, 7.0 * k),
            3.0 * k.max(0.7),
            TINTA_BOTAO,
        );
        if livre && (sobre_mais || capsula.contains(m)) {
            self.aba = ABA_TP;
        }
        if estado.simulado {
            let t = "PAGAMENTO SIMULADO";
            let tam = ts(10.0, k);
            let tw = estilo::medir_forte(t, tam) + 18.0 * k;
            let r = Rect::new(
                capsula.x + capsula.w - tw,
                capsula.y + capsula.h + 6.0 * k,
                tw,
                20.0 * k,
            );
            estilo::ret_arredondado(r, r.h * 0.5, estilo::alfa(AMBAR, 0.16));
            estilo::borda_arredondada(r, r.h * 0.5, 1.0, estilo::alfa(AMBAR, 0.7));
            estilo::texto_centro_forte(r.center().x, r.center().y + 10.0 * k * 0.36, t, tam, AMBAR);
        }
        if !estado.ligada {
            estilo::texto_centro(
                p.center().x,
                p.center().y,
                "A loja está desligada neste servidor.",
                ts(17.0, k),
                estilo::SUAVE,
            );
            return saida;
        }

        // ── abas ──
        let ya = p.y + 104.0 * k;
        let mut xa = xh;
        for (i, (nome, icone)) in ABAS.iter().enumerate() {
            let tam = ts(15.0, k);
            let tw = estilo::medir_forte(nome, tam) + 66.0 * k;
            let r = Rect::new(xa, ya, tw, 44.0 * k);
            let ativa = i == self.aba;
            let sobre = !modal && r.contains(m);
            if ativa {
                estilo::sombra(r, r.h * 0.5, 0.6);
                estilo::ret_gradiente(r, r.h * 0.5, OURO_CLARO, OURO_ESCURO);
                estilo::ret_arredondado(
                    Rect::new(r.x + 4.0 * k, r.y + 3.0 * k, r.w - 8.0 * k, r.h * 0.42),
                    r.h * 0.3,
                    estilo::alfa(WHITE, 0.2),
                );
            } else {
                estilo::ret_arredondado(
                    r,
                    r.h * 0.5,
                    estilo::alfa(WHITE, if sobre { 0.10 } else { 0.05 }),
                );
                estilo::borda_arredondada(
                    r,
                    r.h * 0.5,
                    1.0,
                    estilo::alfa(
                        if sobre { OURO_CLARO } else { LILAS },
                        if sobre { 0.55 } else { 0.25 },
                    ),
                );
            }
            let cor = if ativa { TINTA_BOTAO } else { estilo::TEXTO };
            let ic = vec2(r.x + 25.0 * k, r.center().y);
            if icone.is_empty() {
                estilo::icone_tp(ic, 28.0 * k);
            } else {
                crate::icones_ui::ui(icone, ic, 24.0 * k, cor);
            }
            estilo::texto_forte(
                r.x + 44.0 * k,
                r.center().y + 15.0 * k * 0.36,
                nome,
                tam,
                cor,
            );
            if livre && sobre {
                self.aba = i;
            }
            xa += tw + 10.0 * k;
        }

        let rodape_h = 44.0 * k;
        let area = Rect::new(
            p.x + 24.0 * k,
            ya + 60.0 * k,
            p.w - 48.0 * k,
            p.y + p.h - rodape_h - 12.0 * k - (ya + 60.0 * k),
        );
        match self.aba {
            0 => self.materiais(area, k, m, livre, modal, agora),
            1 => self.aba_pets(area, k, m, livre, modal, agora),
            2 => self.moedas_e_energia(area, k, m, livre, modal, agora),
            _ => self.pacotes(area, k, m, livre, modal, agora),
        }

        // ── rodape ──
        let yr = p.y + p.h - rodape_h;
        draw_rectangle(
            p.x + 24.0 * k,
            yr - 4.0 * k,
            p.w - 48.0 * k,
            1.0,
            estilo::alfa(LILAS, 0.18),
        );
        let base = yr + 26.0 * k;
        if let Some((ok, t)) = &self.ultimo {
            let cor = if *ok { VERDE_POSSE } else { estilo::VERMELHO };
            if *ok {
                visto(vec2(p.x + 36.0 * k, base - 5.0 * k), 12.0 * k, cor);
            } else {
                estilo::texto_forte(p.x + 30.0 * k, base, "!", ts(15.0, k), cor);
            }
            estilo::texto_ajustado(t, p.x + 52.0 * k, base, p.w * 0.42, ts(14.0, k), cor);
        } else {
            estilo::texto(
                p.x + 28.0 * k,
                base,
                "TP é da conta e vale em todos os servidores.",
                ts(13.0, k),
                estilo::alfa(LILAS, 0.8),
            );
        }
        if !estado.historico.is_empty() {
            let hist: Vec<String> = estado
                .historico
                .iter()
                .take(3)
                .map(|c| format!("{} · {}", c.produto, c.valor))
                .collect();
            let t = format!("Últimas compras:  {}", hist.join("   |   "));
            let largura = p.w * 0.50;
            estilo::texto_ajustado(
                &t,
                p.x + p.w - 24.0 * k - largura,
                base,
                largura,
                ts(12.0, k),
                estilo::SUAVE,
            );
        }

        // ── confirmacao ──
        if let Some(conf) = self.confirma {
            // So' vale clique de quando o modal JA' estava aberto: o toque no
            // COMPRAR que abriu a janela cai fora dela e a fechava no mesmo
            // quadro (no iPhone o dialogo "abria e fechava").
            if let Some(msg) =
                self.modal(conf, p, &estado, vox, solido, k, m, clicou && modal, agora)
            {
                saida.push(msg);
            }
        }

        // ── comemoracao ──
        self.comemora(p, k, agora);
        saida
    }

    /// Aba Materiais: pergaminhos repetíveis de chaves e tomos, mais moedas.
    fn materiais(&mut self, area: Rect, k: f32, m: Vec2, livre: bool, modal: bool, agora: f64) {
        let bau = &cat::BAUS_CRAFT[0];
        let vao = 12.0 * k;
        let w = ((area.w - vao * 3.0) / 4.0).min(390.0 * k);
        let total = w * 4.0 + vao * 3.0;
        let x0 = area.center().x - total * 0.5;
        let alto = area.h - 16.0 * k;
        self.pergaminho_de_pet(
            Rect::new(x0 + (w + vao) * 2.0, area.y + 8.0 * k, w, alto),
            k,
            m,
            livre,
            modal,
            agora,
        );
        self.pergaminho_de_montaria(
            Rect::new(x0 + (w + vao) * 3.0, area.y + 8.0 * k, w, alto),
            k,
            m,
            livre,
            modal,
            agora,
        );
        let r = Rect::new(x0, area.y + 8.0 * k, w, area.h - 16.0 * k);
        let sobre = !modal && r.contains(m);
        estilo::sombra(r, 22.0 * k, 1.0);
        estilo::ret_gradiente(
            r,
            22.0 * k,
            Color::new(0.31, 0.18, 0.48, 0.98),
            Color::new(0.055, 0.045, 0.13, 0.98),
        );
        estilo::borda_arredondada(
            r,
            22.0 * k,
            1.5 * k.max(0.8),
            estilo::alfa(if sobre { OURO_CLARO } else { LILAS }, 0.55),
        );
        faiscas(r, agora, 18, k, 93);
        let arte = vec2(r.center().x, r.y + r.h * 0.27);
        brilho_radial(arte, r.h * 0.20, OURO_CLARO, 0.28);
        crate::invocacao_ui::icone_pergaminho_de(
            shared::item_id::PERGAMINHO_INVOCA_CHAVE,
            arte,
            r.h * 0.32,
        );
        estilo::texto_centro_forte(
            r.center().x,
            r.y + r.h * 0.47,
            "Invocação de Chaves",
            ts(22.0, k),
            estilo::TEXTO,
        );
        estilo::texto_centro(
            r.center().x,
            r.y + r.h * 0.53,
            "1 chave aleatória de craft.",
            ts(14.0, k),
            estilo::alfa(LILAS, 0.95),
        );
        let nomes = ["Cinza 55%", "Verde 28%", "Azul 12%", "Roxa 5%"];
        let cores = [
            Color::from_rgba(180, 186, 198, 255),
            Color::from_rgba(88, 220, 125, 255),
            Color::from_rgba(70, 150, 255, 255),
            Color::from_rgba(184, 88, 245, 255),
        ];
        let passo = r.w * 0.20;
        let inicio = r.center().x - passo * 1.5;
        let yc = r.y + r.h * 0.64;
        for i in 0..4 {
            let x = inicio + passo * i as f32;
            draw_circle(x, yc, 12.0 * k, cores[i]);
            draw_circle_lines(x, yc, 14.0 * k, 1.5 * k.max(0.8), estilo::alfa(WHITE, 0.7));
            estilo::texto_centro(x, yc + 29.0 * k, nomes[i], ts(12.0, k), estilo::TEXTO);
        }
        estilo::valor_tp(
            r.center().x - estilo::largura_tp_texto(&milhar(bau.preco_tp), ts(23.0, k), true) * 0.5,
            r.y + r.h - 88.0 * k,
            bau.preco_tp,
            ts(23.0, k),
            OURO_CLARO,
        );
        let bt = Rect::new(
            r.center().x - 120.0 * k,
            r.y + r.h - 62.0 * k,
            240.0 * k,
            48.0 * k,
        );
        let ativo = !self.em_voo;
        botao_ouro(
            bt,
            if self.em_voo {
                "AGUARDE…"
            } else {
                "COMPRAR PERGAMINHO"
            },
            ativo,
            !modal && bt.contains(m),
            k,
            agora,
        );
        if ativo && livre && bt.contains(m) {
            self.confirma = Some(Confirma::Item(Produto::BauCraft(bau.id)));
            self.lote = 1;
        }

        let tomo = &cat::PERGAMINHOS_TOMO[0];
        let r = Rect::new(x0 + w + vao, area.y + 8.0 * k, w, area.h - 16.0 * k);
        let sobre = !modal && r.contains(m);
        estilo::sombra(r, 22.0 * k, 1.0);
        estilo::ret_gradiente(
            r,
            22.0 * k,
            Color::new(0.10, 0.32, 0.30, 0.98),
            Color::new(0.035, 0.09, 0.10, 0.98),
        );
        estilo::borda_arredondada(
            r,
            22.0 * k,
            1.5 * k.max(0.8),
            estilo::alfa(if sobre { OURO_CLARO } else { estilo::VERDE }, 0.60),
        );
        faiscas(r, agora, 18, k, 151);
        let arte = vec2(r.center().x, r.y + r.h * 0.27);
        brilho_radial(arte, r.h * 0.20, estilo::VERDE, 0.28);
        crate::invocacao_ui::icone_pergaminho_de(
            shared::item_id::PERGAMINHO_INVOCA_TOMO,
            arte,
            r.h * 0.32,
        );
        estilo::texto_centro_forte(
            r.center().x,
            r.y + r.h * 0.47,
            "Invocação de Tomos",
            ts(22.0, k),
            estilo::TEXTO,
        );
        estilo::texto_centro(
            r.center().x,
            r.y + r.h * 0.53,
            "1 tomo para uma de 12 habilidades.",
            ts(13.0, k),
            estilo::alfa(estilo::VERDE, 0.95),
        );
        let graus = ["Verde 75%", "Roxo 20%", "Lendário 5%"];
        let cores = [
            Color::from_rgba(88, 220, 125, 255),
            Color::from_rgba(184, 88, 245, 255),
            Color::from_rgba(245, 184, 55, 255),
        ];
        let passo = r.w * 0.27;
        let inicio = r.center().x - passo;
        let yc = r.y + r.h * 0.64;
        for i in 0..3 {
            let x = inicio + passo * i as f32;
            draw_circle(x, yc, 12.0 * k, cores[i]);
            draw_circle_lines(x, yc, 14.0 * k, 1.5 * k.max(0.8), estilo::alfa(WHITE, 0.7));
            estilo::texto_centro(x, yc + 29.0 * k, graus[i], ts(11.0, k), estilo::TEXTO);
        }
        estilo::valor_tp(
            r.center().x - estilo::largura_tp_texto(&milhar(tomo.preco_tp), ts(23.0, k), true) * 0.5,
            r.y + r.h - 88.0 * k,
            tomo.preco_tp,
            ts(23.0, k),
            OURO_CLARO,
        );
        let bt = Rect::new(r.center().x - 108.0 * k, r.y + r.h - 62.0 * k, 216.0 * k, 48.0 * k);
        botao_ouro(
            bt,
            if self.em_voo { "AGUARDE…" } else { "COMPRAR PERGAMINHO" },
            !self.em_voo,
            !modal && bt.contains(m),
            k,
            agora,
        );
        if !self.em_voo && livre && bt.contains(m) {
            self.confirma = Some(Confirma::Item(Produto::PergaminhoTomo(tomo.id)));
            self.lote = 1;
        }
    }

    /// Aba Pets: os consumiveis do pet numa grade. Sao treze (Ração,
    /// Removedor e onze skills), entao o cartao aqui e' COMPACTO — icone em
    /// cima, nome e preco embaixo, e o cartao inteiro e' o botao. O cartao de
    /// lista, com descricao ao lado, nao caberia em cinco colunas.
    fn aba_pets(&mut self, area: Rect, k: f32, m: Vec2, livre: bool, modal: bool, agora: f64) {
        let itens = cat::itens_de_pet();
        let colunas = 5usize;
        let linhas = itens.len().div_ceil(colunas).max(1);
        let vao = 12.0 * k;
        let w = ((area.w - vao * (colunas as f32 - 1.0)) / colunas as f32).min(300.0 * k);
        let h = (area.h - 16.0 * k - vao * (linhas as f32 - 1.0)) / linhas as f32;
        let total = w * colunas as f32 + vao * (colunas as f32 - 1.0);
        let x0 = area.center().x - total * 0.5;
        for (n, item) in itens.iter().enumerate() {
            let r = Rect::new(
                x0 + (n % colunas) as f32 * (w + vao),
                area.y + 8.0 * k + (n / colunas) as f32 * (h + vao),
                w,
                h,
            );
            let sobre = !modal && r.contains(m);
            let racao = item.item_id == shared::item_id::RACAO_DE_PET;
            let remove = item.item_id == shared::item_id::REMOVEDOR_DE_SKILL_PET;
            estilo::sombra(r, 18.0 * k, 1.0);
            estilo::ret_gradiente(
                r,
                18.0 * k,
                if racao {
                    Color::new(0.30, 0.22, 0.10, 0.98)
                } else if remove {
                    Color::new(0.32, 0.13, 0.14, 0.98)
                } else {
                    Color::new(0.14, 0.20, 0.34, 0.98)
                },
                Color::new(0.045, 0.055, 0.13, 0.98),
            );
            estilo::borda_arredondada(
                r,
                18.0 * k,
                1.5 * k.max(0.8),
                estilo::alfa(if sobre { OURO_CLARO } else { LILAS }, 0.5),
            );
            let lado = (r.h * 0.38).min(74.0 * k);
            let ic = vec2(r.center().x, r.y + r.h * 0.30);
            brilho_radial(ic, lado * 0.7, OURO_CLARO, 0.18);
            crate::bolsa::icone_do_item(
                Rect::new(ic.x - lado * 0.5, ic.y - lado * 0.5, lado, lado),
                item.item_id,
                1.0,
            );
            estilo::texto_centro_forte(
                r.center().x,
                r.y + r.h * 0.58,
                item.nome,
                ts(15.0, k),
                estilo::TEXTO,
            );
            estilo::texto_ajustado(
                item.descricao,
                r.x + 10.0 * k,
                r.y + r.h * 0.70,
                r.w - 20.0 * k,
                ts(12.0, k),
                estilo::alfa(LILAS, 0.95),
            );
            let preco = milhar(item.preco_tp);
            estilo::valor_tp(
                r.center().x - estilo::largura_tp_texto(&preco, ts(17.0, k), true) * 0.5,
                r.y + r.h - 16.0 * k,
                item.preco_tp,
                ts(17.0, k),
                OURO_CLARO,
            );
            if !self.em_voo && livre && sobre {
                self.confirma = Some(Confirma::Item(Produto::ItemDePet(item.id)));
            self.lote = 1;
            }
        }
    }

    /// Aba Moedas: o que se compra direto e cai na hora — sacos de moeda e
    /// pacotes de Energia.
    fn moedas_e_energia(
        &mut self,
        area: Rect,
        k: f32,
        m: Vec2,
        livre: bool,
        modal: bool,
        agora: f64,
    ) {
        let vao = 12.0 * k;
        let w = ((area.w - vao) / 2.0).min(440.0 * k);
        let total = w * 2.0 + vao;
        let x0 = area.center().x - total * 0.5;
        let alto = area.h - 16.0 * k;
        self.moedas(
            Rect::new(x0, area.y + 8.0 * k, w, alto),
            k,
            m,
            livre,
            modal,
            agora,
        );
        self.energias(
            Rect::new(x0 + w + vao, area.y + 8.0 * k, w, alto),
            k,
            m,
            livre,
            modal,
            agora,
        );
    }

    /// O Pergaminho de Invocação: Montaria. Espécie e cor saem no ABRIR, como
    /// no de pet — a montaria virou item de bolsa (docs/MONTARIAS.md).
    fn pergaminho_de_montaria(
        &mut self,
        r: Rect,
        k: f32,
        m: Vec2,
        livre: bool,
        modal: bool,
        agora: f64,
    ) {
        let perg = &cat::PERGAMINHOS_MONTARIA[0];
        let sobre = !modal && r.contains(m);
        estilo::sombra(r, 22.0 * k, 1.0);
        estilo::ret_gradiente(
            r,
            22.0 * k,
            Color::new(0.14, 0.24, 0.40, 0.98),
            Color::new(0.04, 0.06, 0.13, 0.98),
        );
        estilo::borda_arredondada(
            r,
            22.0 * k,
            1.5 * k.max(0.8),
            estilo::alfa(if sobre { OURO_CLARO } else { LILAS }, 0.60),
        );
        faiscas(r, agora, 18, k, 307);
        let arte = vec2(r.center().x, r.y + r.h * 0.27);
        brilho_radial(arte, r.h * 0.20, OURO_CLARO, 0.28);
        crate::invocacao_ui::icone_pergaminho_de(
            shared::item_id::PERGAMINHO_INVOCA_MONTARIA,
            arte,
            r.h * 0.32,
        );
        estilo::texto_centro_forte(
            r.center().x,
            r.y + r.h * 0.47,
            "Invocação de Montaria",
            ts(22.0, k),
            estilo::TEXTO,
        );
        estilo::texto_centro(
            r.center().x,
            r.y + r.h * 0.53,
            "1 montaria, espécie sorteada.",
            ts(13.0, k),
            estilo::alfa(LILAS, 0.95),
        );
        let passo = r.w * 0.17;
        let inicio = r.center().x - passo * 2.0;
        let yc = r.y + r.h * 0.64;
        for (i, chance) in shared::montarias::CHANCES_DO_PERGAMINHO.iter().enumerate() {
            let x = inicio + passo * i as f32;
            let cor = cor_do_grau(i as u8 + 1);
            draw_circle(x, yc, 10.0 * k, cor);
            draw_circle_lines(x, yc, 12.0 * k, 1.5 * k.max(0.8), estilo::alfa(WHITE, 0.7));
            estilo::texto_centro(x, yc + 27.0 * k, &format!("{chance}%"), ts(11.0, k), estilo::TEXTO);
        }
        estilo::valor_tp(
            r.center().x
                - estilo::largura_tp_texto(&milhar(perg.preco_tp), ts(23.0, k), true) * 0.5,
            r.y + r.h - 88.0 * k,
            perg.preco_tp,
            ts(23.0, k),
            OURO_CLARO,
        );
        let bt = Rect::new(r.center().x - 108.0 * k, r.y + r.h - 62.0 * k, 216.0 * k, 48.0 * k);
        botao_ouro(
            bt,
            if self.em_voo { "AGUARDE…" } else { "COMPRAR PERGAMINHO" },
            !self.em_voo,
            !modal && bt.contains(m),
            k,
            agora,
        );
        if !self.em_voo && livre && bt.contains(m) {
            self.confirma = Some(Confirma::Item(Produto::PergaminhoMontaria(perg.id)));
            self.lote = 1;
        }
    }

    /// O Pergaminho de Invocação: Pet. Especie e grau saem no ABRIR, entao o
    /// cartao mostra a chance de cada cor, como o de chaves e o de tomos.
    fn pergaminho_de_pet(
        &mut self,
        r: Rect,
        k: f32,
        m: Vec2,
        livre: bool,
        modal: bool,
        agora: f64,
    ) {
        let perg = &cat::PERGAMINHOS_PET[0];
        let sobre = !modal && r.contains(m);
        estilo::sombra(r, 22.0 * k, 1.0);
        estilo::ret_gradiente(
            r,
            22.0 * k,
            Color::new(0.34, 0.20, 0.12, 0.98),
            Color::new(0.10, 0.05, 0.04, 0.98),
        );
        estilo::borda_arredondada(
            r,
            22.0 * k,
            1.5 * k.max(0.8),
            estilo::alfa(if sobre { OURO_CLARO } else { AMBAR }, 0.60),
        );
        faiscas(r, agora, 18, k, 211);
        let arte = vec2(r.center().x, r.y + r.h * 0.27);
        brilho_radial(arte, r.h * 0.20, AMBAR, 0.28);
        crate::invocacao_ui::icone_pergaminho_de(
            shared::item_id::PERGAMINHO_INVOCA_PET,
            arte,
            r.h * 0.32,
        );
        estilo::texto_centro_forte(
            r.center().x,
            r.y + r.h * 0.47,
            "Invocação de Pet",
            ts(22.0, k),
            estilo::TEXTO,
        );
        estilo::texto_centro(
            r.center().x,
            r.y + r.h * 0.53,
            "1 pet coletor, espécie sorteada.",
            ts(13.0, k),
            estilo::alfa(AMBAR, 0.95),
        );
        // As cinco cores e a chance de cada uma, na mesma ordem do catalogo.
        let passo = r.w * 0.17;
        let inicio = r.center().x - passo * 2.0;
        let yc = r.y + r.h * 0.64;
        for (i, chance) in shared::pets::CHANCES_DO_PERGAMINHO.iter().enumerate() {
            let x = inicio + passo * i as f32;
            let cor = cor_do_grau(i as u8 + 1);
            draw_circle(x, yc, 10.0 * k, cor);
            draw_circle_lines(x, yc, 12.0 * k, 1.5 * k.max(0.8), estilo::alfa(WHITE, 0.7));
            estilo::texto_centro(x, yc + 27.0 * k, &format!("{chance}%"), ts(11.0, k), estilo::TEXTO);
        }
        estilo::valor_tp(
            r.center().x
                - estilo::largura_tp_texto(&milhar(perg.preco_tp), ts(23.0, k), true) * 0.5,
            r.y + r.h - 88.0 * k,
            perg.preco_tp,
            ts(23.0, k),
            OURO_CLARO,
        );
        let bt = Rect::new(r.center().x - 108.0 * k, r.y + r.h - 62.0 * k, 216.0 * k, 48.0 * k);
        botao_ouro(
            bt,
            if self.em_voo { "AGUARDE…" } else { "COMPRAR PERGAMINHO" },
            !self.em_voo,
            !modal && bt.contains(m),
            k,
            agora,
        );
        if !self.em_voo && livre && bt.contains(m) {
            self.confirma = Some(Confirma::Item(Produto::PergaminhoPet(perg.id)));
            self.lote = 1;
        }
    }

    /// Um cartao de lista da aba Materiais. A coluna e' estreita, entao o
    /// COMPRAR ocupa a base inteira: ao lado do texto ele cobria o preco.
    #[allow(clippy::too_many_arguments)]
    fn cartao_de_lista(
        &mut self,
        r: Rect,
        k: f32,
        m: Vec2,
        livre: bool,
        modal: bool,
        agora: f64,
        fundo: (Color, Color),
        nome: &str,
        quanto: &str,
        nota: Option<(String, Color)>,
        preco: u64,
        produto: Produto,
        icone: impl FnOnce(Vec2, f32),
    ) {
        let sobre = !modal && r.contains(m);
        estilo::sombra(r, 18.0 * k, 1.0);
        estilo::ret_gradiente(r, 18.0 * k, fundo.0, fundo.1);
        estilo::borda_arredondada(
            r,
            18.0 * k,
            1.5 * k.max(0.8),
            estilo::alfa(if sobre { OURO_CLARO } else { LILAS }, 0.45),
        );
        let lado = (r.h * 0.46).min(76.0 * k);
        let ic = vec2(r.x + 14.0 * k + lado * 0.5, r.y + 44.0 * k);
        brilho_radial(ic, lado * 0.62, OURO_CLARO, 0.18);
        icone(ic, lado);
        let tx = r.x + 14.0 * k + lado + 12.0 * k;
        let largura = r.x + r.w - 12.0 * k - tx;
        estilo::texto_ajustado(nome, tx, r.y + 26.0 * k, largura, ts(16.0, k), estilo::TEXTO);
        estilo::texto_ajustado(
            quanto,
            tx,
            r.y + 46.0 * k,
            largura,
            ts(14.0, k),
            OURO_CLARO,
        );
        let y_preco = r.y + 68.0 * k;
        estilo::valor_tp(tx, y_preco, preco, ts(16.0, k), estilo::TEXTO);
        // O rendimento vai na mesma linha do preco: e' o que compara pacotes.
        if let Some((t, cor)) = nota {
            let x = tx + estilo::largura_tp_texto(&milhar(preco), ts(16.0, k), true) + 8.0 * k;
            estilo::texto_ajustado(&t, x, y_preco, r.x + r.w - 12.0 * k - x, ts(12.0, k), cor);
        }
        let bt = Rect::new(r.x + 12.0 * k, r.y + r.h - 50.0 * k, r.w - 24.0 * k, 40.0 * k);
        let ativo = !self.em_voo;
        botao_ouro(
            bt,
            if self.em_voo { "AGUARDE…" } else { "COMPRAR" },
            ativo,
            !modal && bt.contains(m),
            k,
            agora,
        );
        if ativo && livre && bt.contains(m) {
            self.confirma = Some(Confirma::Item(produto));
            self.lote = 1;
        }
    }

    /// Os pacotes de moeda do jogo, um por linha.
    fn moedas(&mut self, area: Rect, k: f32, m: Vec2, livre: bool, modal: bool, agora: f64) {
        let n = cat::MOEDAS.len() as f32;
        let vao = 12.0 * k;
        let h = (area.h - vao * (n - 1.0)) / n;
        for (i, pk) in cat::MOEDAS.iter().enumerate() {
            let r = Rect::new(area.x, area.y + i as f32 * (h + vao), area.w, h);
            self.cartao_de_lista(
                r,
                k,
                m,
                livre,
                modal,
                agora,
                (
                    Color::new(0.20, 0.15, 0.30, 0.98),
                    Color::new(0.055, 0.045, 0.13, 0.98),
                ),
                pk.nome,
                &format!("{} de uma vez", milhar(pk.qtd as u64)),
                None,
                pk.preco_tp,
                Produto::Moeda(pk.id),
                |c, lado| {
                    crate::bolsa::icone_do_item(
                        Rect::new(c.x - lado * 0.5, c.y - lado * 0.5, lado, lado),
                        pk.item_id,
                        1.0,
                    );
                },
            );
        }
    }

    /// Os pacotes de Energia. Energia nao e' item de bolsa: cai no saldo que
    /// paga tier de habilidade e ponto de atributo, entao o cartao usa o
    /// cristal, nao um icone de item.
    fn energias(&mut self, area: Rect, k: f32, m: Vec2, livre: bool, modal: bool, agora: f64) {
        let n = cat::ENERGIAS.len() as f32;
        let vao = 12.0 * k;
        let h = (area.h - vao * (n - 1.0)) / n;
        let melhor = cat::ENERGIAS
            .iter()
            .map(|e| e.tp_por_mil())
            .fold(f32::MAX, f32::min);
        for (i, pk) in cat::ENERGIAS.iter().enumerate() {
            let r = Rect::new(area.x, area.y + i as f32 * (h + vao), area.w, h);
            let melhor_rendimento = cat::ENERGIAS.len() > 1 && pk.tp_por_mil() <= melhor;
            let nota = (
                format!("{:.1} TP/mil", pk.tp_por_mil()).replace('.', ","),
                if melhor_rendimento {
                    OURO_CLARO
                } else {
                    estilo::SUAVE
                },
            );
            self.cartao_de_lista(
                r,
                k,
                m,
                livre,
                modal,
                agora,
                (
                    Color::new(0.11, 0.24, 0.36, 0.98),
                    Color::new(0.045, 0.055, 0.13, 0.98),
                ),
                pk.nome,
                &format!("{} de Energia", milhar(pk.qtd)),
                Some(nota),
                pk.preco_tp,
                Produto::Energia(pk.id),
                |c, lado| estilo::icone_energia(c, lado),
            );
        }
    }

    /// Aba Tempest Points: os pacotes lado a lado.
    fn pacotes(&mut self, area: Rect, k: f32, m: Vec2, livre: bool, modal: bool, agora: f64) {
        let n = cat::PACOTES.len();
        let colunas = if area.w / area.h > 1.5 { n } else { 2 };
        let linhas = n.div_ceil(colunas);
        let nota_h = 30.0 * k;
        let vao = 16.0 * k;
        let topo_extra = 14.0 * k; // espaco do selo que sai por cima
        let cw = (area.w - vao * (colunas as f32 - 1.0)) / colunas as f32;
        let ch = (area.h - nota_h - topo_extra - vao * (linhas as f32 - 1.0)) / linhas as f32;
        for (i, pk) in cat::PACOTES.iter().enumerate() {
            let r = Rect::new(
                area.x + (i % colunas) as f32 * (cw + vao),
                area.y + topo_extra + (i / colunas) as f32 * (ch + vao),
                cw,
                ch,
            );
            let sobre = !modal && r.contains(m);
            let melhor = selo_do_pacote(pk.id) == Some("MELHOR VALOR");
            let rc = 18.0 * k;
            if melhor {
                estilo::ret_arredondado(
                    Rect::new(r.x - 3.0 * k, r.y - 3.0 * k, r.w + 6.0 * k, r.h + 6.0 * k),
                    rc + 3.0 * k,
                    estilo::alfa(OURO_CLARO, 0.18 + 0.08 * (agora as f32 * 2.0).sin()),
                );
            }
            let sobe = if sobre { 4.0 * k } else { 0.0 };
            let r = Rect::new(r.x, r.y - sobe, r.w, r.h);
            estilo::sombra(r, rc, 1.0);
            estilo::ret_gradiente(
                r,
                rc,
                if melhor {
                    Color::new(0.34, 0.20, 0.50, 0.98)
                } else {
                    Color::new(0.24, 0.16, 0.46, 0.97)
                },
                Color::new(0.06, 0.05, 0.14, 0.97),
            );
            estilo::borda_arredondada(
                r,
                rc,
                if melhor { 2.0 } else { 1.2 } * k.max(0.8),
                if melhor {
                    OURO_CLARO
                } else if sobre {
                    estilo::alfa(OURO_CLARO, 0.55)
                } else {
                    estilo::alfa(LILAS, 0.3)
                },
            );
            let lado = (r.w * 0.62).min(r.h * 0.40);
            let flutua = (agora as f32 * 1.6 + i as f32 * 0.9).sin() * 4.0 * k;
            let arte = vec2(r.center().x, r.y + 30.0 * k + lado * 0.5 + flutua);
            brilho_radial(
                arte,
                lado * 0.72,
                if melhor {
                    Color::new(1.0, 0.75, 0.35, 1.0)
                } else {
                    CIANO
                },
                0.22,
            );
            faiscas(
                Rect::new(r.x, r.y + 10.0 * k, r.w, lado + 40.0 * k),
                agora,
                8,
                k,
                40 + i as u32 * 10,
            );
            crate::icones_ui::loja(&format!("tp_{}", i + 1), arte, lado, 1.0);

            let mut y = r.y + 30.0 * k + lado + 44.0 * k;
            let txt = milhar(pk.total());
            let tam = ts(28.0, k);
            let lw = estilo::largura_tp_texto(&txt, tam, true);
            estilo::tp_texto(r.center().x - lw * 0.5, y, &txt, tam, estilo::TEXTO, true);
            y += 24.0 * k;
            estilo::texto_centro(
                r.center().x,
                y,
                pk.nome,
                ts(14.0, k),
                estilo::alfa(LILAS, 0.95),
            );
            y += 12.0 * k;
            if pk.bonus > 0 {
                let t = format!("+{}% DE BÔNUS", bonus_pct(pk));
                let tam_b = ts(12.0, k);
                let bw = estilo::medir_forte(&t, tam_b) + 22.0 * k;
                let b = Rect::new(r.center().x - bw * 0.5, y, bw, 24.0 * k);
                estilo::ret_gradiente(
                    b,
                    b.h * 0.5,
                    Color::new(0.52, 0.95, 0.66, 1.0),
                    Color::new(0.22, 0.66, 0.42, 1.0),
                );
                estilo::texto_centro_forte(
                    b.center().x,
                    b.center().y + 12.0 * k * 0.36,
                    &t,
                    tam_b,
                    Color::new(0.03, 0.18, 0.08, 1.0),
                );
                estilo::texto_centro(
                    r.center().x,
                    b.y + b.h + 18.0 * k,
                    &format!("{} + {} de bônus", milhar(pk.tp), milhar(pk.bonus)),
                    ts(11.0, k),
                    estilo::SUAVE,
                );
            }
            let bt = Rect::new(
                r.x + 14.0 * k,
                r.y + r.h - 62.0 * k,
                r.w - 28.0 * k,
                48.0 * k,
            );
            let ativo = !self.em_voo;
            botao_ouro(
                bt,
                &cat::preco_brl(pk.centavos),
                ativo,
                !modal && bt.contains(m),
                k,
                agora + i as f64 * 0.7,
            );
            if ativo && livre && sobre {
                self.confirma = Some(Confirma::Tp(pk.id));
            self.lote = 1;
            }
            if let Some(sl) = selo_do_pacote(pk.id) {
                let tam_s = ts(11.0, k);
                let w = estilo::medir_forte(sl, tam_s) + 20.0 * k;
                selo(vec2(r.center().x - w * 0.5, r.y - 11.0 * k), sl, k, true);
            }
        }
        estilo::texto_centro(
            area.center().x,
            area.y + area.h - 8.0 * k,
            "Montarias e skins não dão poder de combate: só estilo e mobilidade.",
            ts(12.0, k),
            estilo::alfa(LILAS, 0.8),
        );
    }

    /// Janela de confirmacao. Devolve o pedido quando confirma.
    #[allow(clippy::too_many_arguments)]
    fn modal(
        &mut self,
        conf: Confirma,
        p: Rect,
        estado: &EstadoLoja,
        vox: &VoxCache,
        solido: &Material,
        k: f32,
        m: Vec2,
        clicou: bool,
        agora: f64,
    ) -> Option<ClientMessage> {
        estilo::ret_arredondado(p, 20.0 * k, Color::new(0.0, 0.0, 0.02, 0.62));
        let mw = (660.0 * k).min(p.w - 40.0 * k);
        let mh = (440.0 * k).min(p.h - 40.0 * k);
        let r = Rect::new(p.center().x - mw * 0.5, p.center().y - mh * 0.5, mw, mh);
        let rc = 20.0 * k;
        brilho_radial(r.center(), mw * 0.65, LILAS, 0.12);
        estilo::sombra(r, rc, 1.4);
        estilo::ret_gradiente(
            r,
            rc,
            Color::new(0.24, 0.15, 0.45, 0.99),
            Color::new(0.06, 0.05, 0.14, 0.99),
        );
        estilo::borda_arredondada(r, rc, 2.0 * k.max(0.8), estilo::alfa(OURO_CLARO, 0.75));
        estilo::texto_centro_forte(
            r.center().x,
            r.y + 40.0 * k,
            "Confirmar compra",
            ts(22.0, k),
            OURO_CLARO,
        );
        draw_rectangle(
            r.x + 40.0 * k,
            r.y + 56.0 * k,
            r.w - 80.0 * k,
            1.0,
            estilo::alfa(LILAS, 0.25),
        );

        // Preview entre o titulo (72) e os botoes (24 + 50 + 24 de respiro).
        let lado = (r.h - 72.0 * k - 98.0 * k).min(r.w * 0.40);
        let prev = Rect::new(r.x + 26.0 * k, r.y + 72.0 * k, lado, lado);
        estilo::ret_gradiente(
            prev,
            16.0 * k,
            Color::new(0.28, 0.19, 0.50, 1.0),
            Color::new(0.07, 0.05, 0.16, 1.0),
        );
        brilho_radial(
            vec2(prev.center().x, prev.y + prev.h * 0.6),
            prev.w * 0.5,
            if matches!(conf, Confirma::Tp(_)) {
                CIANO
            } else {
                LILAS
            },
            0.22,
        );
        estilo::borda_arredondada(prev, 16.0 * k, 1.2 * k.max(0.8), estilo::alfa(LILAS, 0.35));
        let x = prev.x + prev.w + 24.0 * k;
        let largura = r.x + r.w - x - 24.0 * k;
        let mut y = r.y + 96.0 * k;
        let mut insuficiente = false;
        match conf {
            Confirma::Tp(id) => {
                let pk = cat::pacote(id).copied().unwrap_or(cat::PACOTES[0]);
                crate::icones_ui::loja(
                    &format!(
                        "tp_{}",
                        cat::PACOTES.iter().position(|p| p.id == pk.id).unwrap_or(0) + 1
                    ),
                    prev.center(),
                    prev.w * 0.82,
                    1.0,
                );
                estilo::texto_ajustado(pk.nome, x, y, largura, ts(22.0, k), estilo::TEXTO);
                y += 22.0 * k;
                estilo::texto(
                    x,
                    y,
                    "Pacote de Tempest Points",
                    ts(13.0, k),
                    estilo::alfa(LILAS, 0.9),
                );
                y += 40.0 * k;
                estilo::texto(x, y, "Você recebe", ts(13.0, k), estilo::SUAVE);
                estilo::valor_tp(
                    x + largura - estilo::largura_tp_texto(&milhar(pk.total()), ts(22.0, k), true),
                    y + 4.0 * k,
                    pk.total(),
                    ts(22.0, k),
                    OURO_CLARO,
                );
                y += 38.0 * k;
                estilo::texto(x, y, "Preço", ts(13.0, k), estilo::SUAVE);
                let preco = cat::preco_brl(pk.centavos);
                estilo::texto_forte(
                    x + largura - estilo::medir_forte(&preco, ts(22.0, k)),
                    y + 4.0 * k,
                    &preco,
                    ts(22.0, k),
                    estilo::TEXTO,
                );
                if estado.simulado {
                    y += 32.0 * k;
                    estilo::texto_ajustado(
                        "Pagamento simulado: nada é cobrado.",
                        x,
                        y,
                        largura,
                        ts(12.0, k),
                        AMBAR,
                    );
                }
            }
            Confirma::Item(pr) => {
                let skin = match pr {
                    Produto::Tp(_)
                    | Produto::BauCraft(_)
                    | Produto::Moeda(_)
                    | Produto::PergaminhoMontaria(_)
                    | Produto::PergaminhoTomo(_)
                    | Produto::Energia(_)
                    | Produto::PergaminhoPet(_)
                    | Produto::ItemDePet(_) => 0,
                };
                if matches!(
                    pr,
                    Produto::BauCraft(_)
                        | Produto::PergaminhoMontaria(_)
                        | Produto::PergaminhoTomo(_)
                        | Produto::PergaminhoPet(_)
                ) {
                    brilho_radial(prev.center(), prev.w * 0.42, OURO_CLARO, 0.32);
                    let item_id = match pr {
                        Produto::PergaminhoMontaria(_) => {
                            shared::item_id::PERGAMINHO_INVOCA_MONTARIA
                        }
                        Produto::PergaminhoTomo(_) => shared::item_id::PERGAMINHO_INVOCA_TOMO,
                        Produto::PergaminhoPet(_) => shared::item_id::PERGAMINHO_INVOCA_PET,
                        _ => shared::item_id::PERGAMINHO_INVOCA_CHAVE,
                    };
                    crate::invocacao_ui::icone_pergaminho_de(
                        item_id,
                        prev.center(),
                        prev.w * 0.58,
                    );
                } else if let Produto::ItemDePet(id) = pr {
                    brilho_radial(prev.center(), prev.w * 0.42, OURO_CLARO, 0.32);
                    if let Some(x) = cat::item_de_pet(id) {
                        let l = prev.w * 0.5;
                        crate::bolsa::icone_do_item(
                            Rect::new(prev.center().x - l * 0.5, prev.center().y - l * 0.5, l, l),
                            x.item_id,
                            1.0,
                        );
                    }
                } else if let Produto::Energia(_) = pr {
                    brilho_radial(
                        prev.center(),
                        prev.w * 0.42,
                        Color::new(0.35, 0.8, 1.0, 1.0),
                        0.34,
                    );
                    estilo::icone_energia(prev.center(), prev.w * 0.5);
                } else if let Produto::Moeda(id) = pr {
                    brilho_radial(prev.center(), prev.w * 0.42, OURO_CLARO, 0.32);
                    if let Some(mo) = cat::moeda(id) {
                        let l = prev.w * 0.5;
                        crate::bolsa::icone_do_item(
                            Rect::new(prev.center().x - l * 0.5, prev.center().y - l * 0.5, l, l),
                            mo.item_id,
                            1.0,
                        );
                    }
                } else {
                    self.giro += get_frame_time().min(0.1) * 0.6;
                    crate::render3d::vitrine_montaria(
                        vox,
                        skin,
                        Rect::new(
                            prev.x + 6.0 * k,
                            prev.y + 6.0 * k,
                            prev.w - 12.0 * k,
                            prev.h - 12.0 * k,
                        ),
                        self.giro,
                        solido,
                    );
                }
                let tipo = match pr {
                    Produto::Tp(_) => String::new(),
                    Produto::BauCraft(_) => {
                        "Pergaminho · abra na bolsa · chave aleatória".to_string()
                    }
                    Produto::PergaminhoMontaria(_) => {
                        "Pergaminho · 1 montaria · espécie e cor sorteadas".to_string()
                    }
                    Produto::PergaminhoTomo(_) => {
                        "Pergaminho · tomo aleatório para 1 de 12 habilidades".to_string()
                    }
                    Produto::Moeda(id) => cat::moeda(id).map_or(String::new(), |mo| {
                        format!("{} · entra na hora", milhar(mo.qtd as u64))
                    }),
                    Produto::Energia(id) => cat::energia(id).map_or(String::new(), |e| {
                        format!("{} de Energia · entra na hora", milhar(e.qtd))
                    }),
                    Produto::PergaminhoPet(_) => {
                        "Pergaminho · 1 pet coletor · espécie e grau sorteados".to_string()
                    }
                    Produto::ItemDePet(id) => {
                        cat::item_de_pet(id).map_or(String::new(), |x| x.descricao.to_string())
                    }
                };
                estilo::texto_ajustado(&pr.nome(), x, y, largura, ts(22.0, k), estilo::TEXTO);
                y += 22.0 * k;
                estilo::texto_ajustado(&tipo, x, y, largura, ts(13.0, k), estilo::alfa(LILAS, 0.9));
                // ── quantas de uma vez ──
                //
                // So' pra item: pacote de TP e' compra de dinheiro de verdade,
                // e lote ali e' cobranca repetida sem querer.
                let unitario = pr.preco_tp().unwrap_or(0);
                let lote = cat::lote(self.lote);
                let sel = Rect::new(x, y + 24.0 * k, largura, 40.0 * k);
                let lado = 40.0 * k;
                let menos = Rect::new(sel.x, sel.y, lado, lado);
                let mais = Rect::new(sel.x + lado + 8.0 * k, sel.y, lado, lado);
                for (b, t, liga) in [
                    (menos, "−", lote > 1),
                    (mais, "+", lote < cat::LOTE_MAX),
                ] {
                    botao_contorno(b, t, liga && b.contains(m), k);
                    if clicou && liga && b.contains(m) {
                        self.lote = if t == "−" { lote - 1 } else { lote + 1 };
                    }
                }
                estilo::texto_centro_forte(
                    mais.x + lado + 30.0 * k,
                    sel.y + 27.0 * k,
                    &format!("{lote}x"),
                    ts(20.0, k),
                    estilo::TEXTO,
                );
                // Atalhos: o lote grande se faz num toque, nao em dez.
                let mut ax = mais.x + lado + 62.0 * k;
                for n in [1u16, 10, 50] {
                    let b = Rect::new(ax, sel.y + 4.0 * k, 46.0 * k, lado - 8.0 * k);
                    botao_contorno(b, &format!("{n}x"), b.contains(m), k);
                    if clicou && b.contains(m) {
                        self.lote = n;
                    }
                    ax += 52.0 * k;
                }
                // O desconto ao lado dos atalhos: e' tocando no "10x" e vendo
                // o "−10%" aparecer que a regra se aprende, sem texto.
                let pct = cat::desconto_pct(lote);
                if pct > 0 {
                    estilo::texto_forte(
                        ax + 6.0 * k,
                        sel.y + 27.0 * k,
                        &format!("−{pct}%"),
                        ts(17.0, k),
                        estilo::VERDE,
                    );
                }
                y = sel.y + lado - 24.0 * k;
                let cheio = unitario.saturating_mul(lote as u64);
                let preco = cat::preco_do_lote(unitario, lote);
                let tam = ts(20.0, k);
                let linha = |y: f32, rotulo: &str, v: u64, cor: Color| {
                    estilo::texto(x, y, rotulo, ts(13.0, k), estilo::SUAVE);
                    estilo::valor_tp(
                        x + largura - estilo::largura_tp_texto(&milhar(v), tam, true),
                        y + 4.0 * k,
                        v,
                        tam,
                        cor,
                    );
                };
                y += 40.0 * k;
                linha(
                    y,
                    &if lote > 1 {
                        format!("Preço · {lote}x {}", milhar(unitario))
                    } else {
                        "Preço".to_string()
                    },
                    preco,
                    OURO_CLARO,
                );
                // O que ele deixou de pagar, riscado: sem isso o desconto e'
                // so' um numero menor, e numero menor ninguem compara de
                // cabeca com o que teria sido.
                if pct > 0 {
                    let velho = milhar(cheio);
                    let t = ts(13.0, k);
                    let w = estilo::medir(&velho, t);
                    let vx = x + largura - estilo::largura_tp_texto(&milhar(preco), tam, true) - w
                        - 12.0 * k;
                    estilo::texto(vx, y - 10.0 * k, &velho, t, estilo::SUAVE);
                    draw_line(
                        vx,
                        y - 14.0 * k,
                        vx + w,
                        y - 14.0 * k,
                        1.5 * k,
                        estilo::SUAVE,
                    );
                }
                y += 34.0 * k;
                linha(y, "Seu saldo", estado.tp, estilo::TEXTO);
                y += 34.0 * k;
                match saldo_apos(estado.tp, preco) {
                    Some(v) => linha(y, "Após a compra", v, estilo::TEXTO),
                    None => {
                        insuficiente = true;
                        estilo::texto_forte(
                            x,
                            y,
                            "Saldo insuficiente",
                            ts(15.0, k),
                            estilo::VERMELHO,
                        );
                    }
                }
            }
        }

        let bw = (r.w - 26.0 * k * 3.0) * 0.5;
        let bh = 50.0 * k;
        let cancelar = Rect::new(r.x + 26.0 * k, r.y + r.h - 24.0 * k - bh, bw, bh);
        let confirmar = Rect::new(cancelar.x + bw + 26.0 * k, cancelar.y, bw, bh);
        botao_contorno(cancelar, "Cancelar", cancelar.contains(m), k);
        let rotulo = if insuficiente {
            "COMPRAR TP"
        } else {
            "CONFIRMAR"
        };
        botao_ouro(confirmar, rotulo, true, confirmar.contains(m), k, agora);
        if !clicou {
            return None;
        }
        if confirmar.contains(m) {
            self.confirma = None;
            if insuficiente {
                self.aba = ABA_TP;
                return None;
            }
            let pedido = self.novo_pedido();
            self.em_voo = true;
            return Some(ClientMessage::Loja {
                pedido: match conf {
                    Confirma::Tp(pacote) => PedidoLoja::ComprarTp { pacote, pedido },
                    Confirma::Item(produto) => PedidoLoja::ComprarItem {
                        produto,
                        vezes: cat::lote(self.lote),
                        pedido,
                    },
                },
            });
        }
        if cancelar.contains(m) || !r.contains(m) {
            self.confirma = None;
        }
        None
    }

    /// Brilho, raios e confete quando uma compra da' certo.
    fn comemora(&mut self, p: Rect, k: f32, agora: f64) {
        const DURACAO: f32 = 2.4;
        let Some((t0, texto)) = self.festa.clone() else {
            return;
        };
        let e = (agora - t0) as f32;
        if e > DURACAO {
            self.festa = None;
            return;
        }
        let some = 1.0 - (e / DURACAO).powf(2.0);
        let c = vec2(p.center().x, p.center().y - 20.0 * k);
        brilho_radial(c, p.h * 0.45 * (0.6 + e.min(0.4)), OURO_CLARO, 0.35 * some);
        for i in 0..18 {
            let a = i as f32 / 18.0 * TAU + e * 0.6;
            let len = p.h * (0.18 + 0.20 * e.min(1.0)) * (0.8 + 0.4 * sorteio(i));
            let de = c + vec2(a.cos(), a.sin()) * 40.0 * k;
            let ate = c + vec2(a.cos(), a.sin()) * len;
            estilo::traco(
                de,
                ate,
                3.0 * k.max(0.7),
                estilo::alfa(OURO_CLARO, 0.28 * some),
            );
        }
        let cores = [OURO_CLARO, LILAS, CIANO, VERDE_POSSE, AMBAR];
        for i in 0..56u32 {
            let a = sorteio(i) * TAU;
            let vel = (160.0 + 360.0 * sorteio(i + 101)) * k;
            let pos = c + vec2(a.cos(), a.sin() * 0.7) * vel * e + vec2(0.0, 300.0 * k * e * e);
            let cor = estilo::alfa(cores[(i % 5) as usize], some);
            draw_poly(
                pos.x,
                pos.y,
                4,
                (3.0 + 3.0 * sorteio(i + 7)) * k.max(0.6),
                e * 400.0 * (sorteio(i + 3) - 0.5),
                cor,
            );
        }
        let pop = (e * 6.0).min(1.0);
        let escala = 0.75 + 0.25 * pop + (1.0 - pop) * 0.2;
        estilo::texto_sombra(
            c.x - estilo::medir_forte("Compra concluída!", ts(34.0 * escala, k)) * 0.5,
            c.y,
            "Compra concluída!",
            ts(34.0 * escala, k),
            estilo::alfa(OURO_CLARO, some.max(0.0)),
            true,
        );
        estilo::texto_centro(
            c.x,
            c.y + 32.0 * k,
            &texto,
            ts(15.0, k),
            estilo::alfa(estilo::TEXTO, some.max(0.0)),
        );
    }
}


// ─────────────────────────────── previa offscreen ───────────────────────────────

/// `MMO_PREVIA_LOJA=1`: abre a loja com dados falsos e salva capturas PNG de
/// cada aba, da confirmacao e da comemoracao em `MMO_PREVIA_SAIDA`
/// (padrao /tmp/2dengine-loja). `MMO_PREVIA_UI` = escala da interface,
/// `MMO_PREVIA_MARGENS` = "topo,esq,baixo,dir" (area segura do iPhone).
pub async fn previa(vox: &VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/2dengine-loja".into());
    let _ = std::fs::create_dir_all(&saida);
    if let Some(v) = std::env::var("MMO_PREVIA_UI")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
    {
        crate::hud_layout::define_escala_ui(v);
    }
    if let Ok(mg) = std::env::var("MMO_PREVIA_MARGENS") {
        let v: Vec<f32> = mg
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if v.len() == 4 {
            crate::hud_layout::define_margens([v[0], v[1], v[2], v[3]]);
        }
    }
    // Captura em TEXTURA, nao na tela: `get_screen_data` le' o buffer da
    // FRENTE, e sem tela (Xvfb + llvmpipe) ele nunca e' resolvido — a imagem
    // saia branca e do tamanho da janela, nao do pedido. Com alvo, o tamanho
    // e' o do alvo e o conteudo existe sempre.
    let (lw, lh) = tam_da_previa();
    let rt = macroquad::texture::render_target_ex(
        lw,
        lh,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let mut loja = LojaTp::default();
    let _ = loja.abrir();
    let estado = EstadoLoja {
        ligada: true,
        simulado: true,
        tp: 1_250,
        historico: vec![
            cat::CompraNet {
                produto: "Bolsa de TP".into(),
                valor: "R$ 24,90".into(),
                status: "creditado".into(),
                quando_unix: 0,
            },
            cat::CompraNet {
                produto: "Lobo da Clareira".into(),
                valor: "500 TP".into(),
                status: "entregue".into(),
                quando_unix: 0,
            },
            cat::CompraNet {
                produto: "Lobo da Meia-Noite".into(),
                valor: "300 TP".into(),
                status: "entregue".into(),
                quando_unix: 0,
            },
        ],
    };
    loja.receber(AvisoLoja::Estado(estado.clone()));
    let cenas: [&str; 7] = [
        "1-materiais",
        "2-pets",
        "3-moedas",
        "4-tempest-points",
        "5-confirma-pergaminho",
        "6-confirma-tp",
        "7-compra-concluida",
    ];
    for (n, cena) in cenas.iter().enumerate() {
        loja.confirma = None;
        loja.festa = None;
        loja.festa_pendente = None;
        loja.ultimo = None;
        match n {
            0 => loja.aba = 0,
            1 => loja.aba = 1,
            2 => loja.aba = 2,
            3 => loja.aba = ABA_TP,
            4 => {
                loja.aba = 0;
                loja.confirma = Some(Confirma::Item(Produto::PergaminhoMontaria(1)));
            }
            5 => {
                loja.aba = ABA_TP;
                loja.confirma = Some(Confirma::Tp(4));
            }
            _ => {
                loja.aba = 0;
                loja.receber(AvisoLoja::Resultado {
                    ok: true,
                    texto: "Pergaminho de Invocação: Montaria entregue na bolsa!".into(),
                });
            }
        }
        let (sw, sh) = (lw, lh);
        for quadro in 0..48 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.10, 0.14, 0.12, 1.0));
            // Um "mundo" atras, pra ver o escurecido do painel.
            let (lsw, lsh) = crate::render3d::tela();
            for i in 0..14 {
                let x = (i as f32 * 0.137).fract() * lsw;
                draw_circle(
                    x,
                    lsh * (0.3 + 0.5 * ((i as f32 * 0.71).fract())),
                    60.0 + i as f32 * 6.0,
                    Color::new(0.20, 0.32, 0.22, 1.0),
                );
            }
            let _ = loja.desenha(vox, &solido);
            if quadro == if n == 6 { 22 } else { 46 } {
                salva_alvo(&rt, &format!("{saida}/{sw}x{sh}-{cena}.png"));
            }
            next_frame().await;
        }
    }
}

/// Prova visual pelo caminho do MUNDO (`draw_entities`): os quadrupedes em
/// pecas (lobo, urso, tigre, owlbear), um chefe que reusa o corpo do lobo e
/// um cavaleiro montado no tigre — parados e em tres quadros da corrida — e a
/// vitrine do mesmo tigre ao lado. Salva `{prefixo}-mundo-*.png`.
async fn previa_mundo(vox: &VoxCache, solido: &Material, prefixo: &str) {
    use shared::{EntityId, EntityMeta, EntityState, EntityTag};
    let mut mundo = crate::world::World::default();
    let mut metas = Vec::new();
    let mut estados = Vec::new();
    // (id, tag, kind, pos, flags)
    let elenco: [(u32, EntityTag, u16, Vec2, u8); 6] = [
        (1, EntityTag::Enemy, 0, vec2(-7.5, 0.0), 0),
        (2, EntityTag::Enemy, 1, vec2(-4.2, 0.0), 0),
        (3, EntityTag::Enemy, 3, vec2(-0.9, 0.0), 0),
        (4, EntityTag::Enemy, 5, vec2(2.6, 0.0), 0),
        (
            5,
            EntityTag::Enemy,
            10,
            vec2(8.5, 2.5),
            shared::ent_flags::BOSS,
        ),
        (
            6,
            EntityTag::Player,
            shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 4),
            vec2(-2.5, -4.2),
            shared::ent_flags::MONTADO,
        ),
    ];
    for (id, tag, kind, p, flags) in elenco {
        metas.push(EntityMeta {
            id: EntityId(id),
            tag,
            name: None,
            hp_max: 100,
            faction: None,
            kind,
            nivel: 10,
        });
        estados.push(EntityState::quantize(
            EntityId(id),
            ::glam::Vec2::new(p.x, p.y),
            ::glam::Vec2::ZERO,
            100,
            flags,
        ));
    }
    mundo.apply(metas, estados, &[]);
    // (nome, andando, fase da passada)
    let tomadas: [(&str, bool, f32); 4] = [
        ("parado", false, 0.0),
        ("corrida-1", true, 0.0),
        ("corrida-2", true, 1.6),
        ("corrida-3", true, 3.2),
    ];
    for (nome, anda, fase) in tomadas {
        for quadro in 0..30 {
            mundo.tick(get_frame_time(), &|_, _| 0.0);
            for e in mundo.ents.values_mut() {
                e.yaw = std::f32::consts::FRAC_PI_2;
                e.andar = if anda { 1.0 } else { 0.0 };
                e.fase = fase;
            }
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.10, 0.12, 1.0));
            let (sw, sh) = crate::render3d::tela();
            let esquerda = Rect::new(0.0, 0.0, sw * 0.70, sh);
            let mut vista =
                crate::render3d::Vista::nova(vec2(0.0, 1.0), 0.0, 0.4, 0.8, 0.0, &|_, _| 0.0);
            vista.cam.position = vec3(0.4, 6.5, -17.0);
            vista.cam.target = vec3(0.4, 1.0, 0.0);
            vista.cam.viewport = crate::render3d::viewport_na_tela(esquerda);
            vista.cam.aspect = Some(esquerda.w / esquerda.h);
            vista.cam.render_target = crate::render3d::alvo();
            set_camera(&vista.cam);
            crate::render3d::limpa_so_profundidade();
            draw_plane(
                vec3(0.0, 0.0, 0.0),
                vec2(30.0, 30.0),
                None,
                Color::new(0.20, 0.24, 0.22, 1.0),
            );
            macroquad::material::gl_use_material(solido);
            solido.set_uniform("Recorte", Vec3::ZERO);
            crate::render3d::draw_entities(&mut mundo, vox, None, &vista, false);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            let direita = Rect::new(sw * 0.72, sh * 0.2, sw * 0.26, sh * 0.55);
            draw_rectangle(
                direita.x,
                direita.y,
                direita.w,
                direita.h,
                Color::new(0.16, 0.11, 0.30, 1.0),
            );
            if let Some(c) = crate::render3d::vitrine_chao(vox, 201, direita) {
                draw_ellipse(
                    c.x,
                    c.y,
                    direita.w * 0.3,
                    direita.h * 0.05,
                    0.0,
                    estilo::alfa(OURO_CLARO, 0.6),
                );
            }
            crate::render3d::vitrine_montaria(vox, 201, direita, 2.2, solido);
            estilo::texto(16.0, 30.0, &format!("MUNDO · {nome} · lobo, urso, tigre, owlbear, chefe Lobo Alfa, cavaleiro no tigre"), 15, WHITE);
            estilo::texto(direita.x, direita.y - 10.0, "VITRINE", 15, WHITE);
            if quadro == 28 {
                if let Some(rt) = crate::render3d::alvo() {
                    salva_alvo(&rt, &format!("{prefixo}-mundo-{nome}.png"));
                }
            }
            next_frame().await;
        }
    }
    previa_montado(vox, solido, &mut mundo, prefixo).await;
}

/// O CAVALEIRO de perto: e' a pose que esta' em ajuste. De lado e de tras,
/// parado e andando, com a camera colada — no plano geral ele sai pequeno
/// demais pra julgar se a perna entra no bicho.
async fn previa_montado(
    vox: &VoxCache,
    solido: &Material,
    mundo: &mut crate::world::World,
    prefixo: &str,
) {
    // (nome, angulo da camera em volta do cavaleiro, andando)
    let tomadas: [(&str, f32, bool); 4] = [
        ("lado", 0.0, false),
        ("lado-andando", 0.0, true),
        ("tras", std::f32::consts::FRAC_PI_2, false),
        ("tras-andando", std::f32::consts::FRAC_PI_2, true),
    ];
    let foco = vec2(-2.5, -4.2);
    for (nome, ang, anda) in tomadas {
        for quadro in 0..30 {
            mundo.tick(get_frame_time(), &|_, _| 0.0);
            for e in mundo.ents.values_mut() {
                e.yaw = std::f32::consts::FRAC_PI_2;
                e.andar = if anda { 1.0 } else { 0.0 };
            }
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.10, 0.12, 1.0));
            let (sw, sh) = crate::render3d::tela();
            let tudo = Rect::new(0.0, 0.0, sw, sh);
            let mut vista = crate::render3d::Vista::nova(foco, 0.0, 0.4, 0.8, 0.0, &|_, _| 0.0);
            vista.cam.position = vec3(foco.x + 4.2 * ang.cos(), 2.1, foco.y + 4.2 * ang.sin());
            vista.cam.target = vec3(foco.x, 1.25, foco.y);
            vista.cam.viewport = crate::render3d::viewport_na_tela(tudo);
            vista.cam.aspect = Some(sw / sh);
            vista.cam.render_target = crate::render3d::alvo();
            set_camera(&vista.cam);
            crate::render3d::limpa_so_profundidade();
            draw_plane(
                vec3(0.0, 0.0, 0.0),
                vec2(30.0, 30.0),
                None,
                Color::new(0.20, 0.24, 0.22, 1.0),
            );
            macroquad::material::gl_use_material(solido);
            solido.set_uniform("Recorte", Vec3::ZERO);
            crate::render3d::draw_entities(mundo, vox, None, &vista, false);
            macroquad::material::gl_use_default_material();
            crate::render3d::camera_padrao();
            estilo::texto(
                16.0,
                30.0,
                &format!("MONTADO · {nome} · cavaleiro no tigre"),
                15,
                WHITE,
            );
            if quadro == 28 {
                if let Some(rt) = crate::render3d::alvo() {
                    salva_alvo(&rt, &format!("{prefixo}-montado-{nome}.png"));
                }
            }
            next_frame().await;
        }
    }
}

/// Tamanho da captura: `MMO_PREVIA_TAM="LxA"`, padrao o do iPhone deitado.
fn tam_da_previa() -> (u32, u32) {
    std::env::var("MMO_PREVIA_TAM")
        .ok()
        .and_then(|s| {
            let (a, b) = s.split_once('x')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .unwrap_or((2532, 1170))
}

/// Salva o alvo em PNG. `get_texture_data` ja' devolve de cima pra baixo:
/// virar de novo sairia de cabeca pra baixo (ja' aconteceu).
fn salva_alvo(rt: &macroquad::texture::RenderTarget, caminho: &str) {
    unsafe { get_internal_gl().flush() };
    rt.texture.get_texture_data().export_png(caminho);
}

#[cfg(test)]
mod tests {
    use super::*;


    fn estado(tp: u64) -> EstadoLoja {
        EstadoLoja {
            ligada: true,
            tp,
            ..Default::default()
        }
    }

    #[test]
    fn pedidos_novos_sao_validos_e_diferentes() {
        let mut l = LojaTp::default();
        let a = l.novo_pedido();
        let b = l.novo_pedido();
        assert!(cat::pedido_valido(&a), "{a}");
        assert_ne!(a, b);
    }



    #[test]
    fn selos_bonus_e_saldo() {
        assert_eq!(selo_do_pacote(4), Some("MELHOR VALOR"));
        assert_eq!(selo_do_pacote(1), None);
        assert_eq!(bonus_pct(cat::pacote(2).unwrap()), 10);
        assert_eq!(bonus_pct(cat::pacote(4).unwrap()), 30);
        assert_eq!(bonus_pct(cat::pacote(1).unwrap()), 0);
        assert_eq!(saldo_apos(1_250, 800), Some(450));
        assert_eq!(saldo_apos(100, 800), None);
    }

    #[test]
    fn saldo_animado_chega_no_alvo() {
        let mut v = 0.0;
        for _ in 0..200 {
            v = anima_saldo(v, 550.0, 1.0 / 30.0);
        }
        assert_eq!(v, 550.0);
    }

    #[test]
    fn resultado_vai_pro_chat_libera_o_botao_e_comemora() {
        let mut l = LojaTp {
            em_voo: true,
            ..Default::default()
        };
        assert_eq!(
            l.receber(AvisoLoja::Resultado {
                ok: true,
                texto: "ok".into()
            }),
            Some("Loja: ok".into())
        );
        assert!(!l.em_voo);
        assert!(l.festa_pendente.is_some());
        l.festa_pendente = None;
        l.receber(AvisoLoja::Resultado {
            ok: false,
            texto: "sem saldo".into(),
        });
        assert!(l.festa_pendente.is_none(), "recusa nao comemora");
    }

    #[test]
    fn primeiro_estado_nao_anima_o_saldo() {
        let mut l = LojaTp::default();
        l.receber(AvisoLoja::Estado(estado(900)));
        assert_eq!(l.tp_mostrado, 900.0);
    }
}

/// A cor do grau do pet, da mesma tabela de cor dos itens.
fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}
