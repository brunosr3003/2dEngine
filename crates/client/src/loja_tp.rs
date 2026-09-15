//! Loja de cash (docs/LOJA.md): Menu → Comércio → Loja. Três abas —
//! Montarias, Skins de montaria e Tempest Points — com o saldo de TP no topo.
//!
//! Tudo que vale e' do servidor: a janela so' mostra o catalogo
//! (`shared::loja`) e pede. Cada compra leva um id de pedido novo, gerado
//! aqui: clique duplo e reenvio valem uma compra so' no banco.
use macroquad::prelude::*;
use shared::loja::{self as cat, AvisoLoja, EstadoLoja, PedidoLoja, Produto};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

const ABAS: [&str; 3] = ["Montarias", "Skins", "Tempest Points"];
const COLUNAS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Confirma {
    Tp(u16),
    Item(Produto),
}

#[derive(Default)]
pub struct LojaTp {
    pub aberto: bool,
    estado: Option<EstadoLoja>,
    aba: usize,
    confirma: Option<Confirma>,
    /// Ultimo resultado (ok, texto), no rodape.
    ultimo: Option<(bool, String)>,
    /// Compra enviada e sem resposta: o botao fica "Aguarde".
    em_voo: bool,
    contador: u32,
}

/// A cor do bicho de uma montaria, pro cartao (a skin padrao nao tinge).
pub fn cor_da_montaria(id: u16) -> Color {
    match id {
        1 => Color::from_rgba(150, 152, 162, 255),
        2 => Color::from_rgba(228, 164, 72, 255),
        _ => Color::from_rgba(126, 86, 54, 255),
    }
}

/// A cor de uma skin: a da montaria puxada pra tinta, como no 3D.
pub fn cor_da_skin(id: u16) -> Color {
    let Some(s) = cat::skin(id) else { return GRAY };
    let base = cor_da_montaria(s.montaria);
    let t = Color::from_rgba(s.tinta[0], s.tinta[1], s.tinta[2], 255);
    estilo::misturar(base, t, s.forca)
}

/// Um medalhao redondo com a cor e a inicial da montaria.
pub fn medalhao(c: Vec2, r: f32, cor: Color, letra: &str) {
    draw_circle(c.x, c.y, r, estilo::alfa(cor, 0.25));
    draw_circle(c.x, c.y, r * 0.78, cor);
    draw_circle_lines(c.x, c.y, r, 2.0, estilo::alfa(cor, 0.9));
    estilo::texto_centro_forte(c.x, c.y + r * 0.3, letra, (r * 0.9).clamp(12.0, 40.0) as u16, Color::from_rgba(20, 18, 24, 230));
}

/// "1.250".
fn milhar(v: u64) -> String {
    crate::economia::milhar(v)
}

impl LojaTp {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.confirma = None;
        vec![ClientMessage::Loja { pedido: PedidoLoja::Estado }]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.confirma = None;
    }

    pub fn estado(&self) -> Option<&EstadoLoja> {
        self.estado.as_ref()
    }

    /// Chegou aviso do servidor. Devolve texto pro chat.
    pub fn receber(&mut self, aviso: AvisoLoja) -> Option<String> {
        match aviso {
            AvisoLoja::Estado(e) => {
                self.estado = Some(e);
                self.em_voo = false;
                None
            }
            AvisoLoja::Resultado { ok, texto } => {
                self.em_voo = false;
                self.ultimo = Some((ok, texto.clone()));
                Some(format!("Loja: {texto}"))
            }
            AvisoLoja::Montando { .. } => None,
        }
    }

    /// Id de pedido novo: relogio em nanossegundos + contador. Cabe nas regras
    /// de `shared::loja::pedido_valido`.
    fn novo_pedido(&mut self) -> String {
        self.contador = self.contador.wrapping_add(1);
        let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let id = format!("c{ns:x}-{:x}", self.contador);
        debug_assert!(cat::pedido_valido(&id));
        id
    }

    pub fn desenha(&mut self) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (920.0 * f).min(seguro.w - 16.0);
        let h = (660.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        // Com a confirmacao aberta, o toque so' vale nela.
        let clicou_livre = is_mouse_button_pressed(MouseButton::Left) && self.confirma.is_none();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Loja", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 7.0 * f, "X", 18, estilo::TEXTO);
        if clicou_livre && fechar.contains(m) {
            self.fechar();
            return saida;
        }
        let Some(estado) = self.estado.clone() else {
            estilo::texto(x0, p.y + 80.0 * f, "Carregando…", 15, estilo::SUAVE);
            return saida;
        };
        // Saldo e aviso de pagamento simulado, no topo.
        let saldo = format!("{} TP", milhar(estado.tp));
        let sw = estilo::medir_forte(&saldo, 18);
        estilo::texto_forte(fechar.x - 16.0 * f - sw, p.y + 36.0 * f, &saldo, 18, estilo::TEXTO);
        if estado.simulado {
            let t = "PAGAMENTO SIMULADO";
            let tw = estilo::medir_forte(t, 11) + 16.0 * f;
            let r = Rect::new(x0 + estilo::medir_forte("Loja", 22) + 16.0 * f, p.y + 18.0 * f, tw, 24.0 * f);
            estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, Color::new(0.85, 0.55, 0.15, 0.25));
            estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 1.0, Color::new(0.95, 0.65, 0.2, 0.9));
            estilo::texto_centro_forte(r.center().x, r.center().y + 4.0 * f, t, 11, Color::new(1.0, 0.78, 0.35, 1.0));
        }
        if !estado.ligada {
            estilo::texto(x0, p.y + 90.0 * f, "A loja está desligada neste servidor.", 15, estilo::SUAVE);
            return saida;
        }

        // Abas.
        let mut x = x0;
        let ya = p.y + 54.0 * f;
        for (i, nome) in ABAS.iter().enumerate() {
            let tw = estilo::medir(nome, 15) + 32.0 * f;
            let r = Rect::new(x, ya, tw, 38.0 * f);
            estilo::cartao(r, r.contains(m), i == self.aba);
            estilo::texto_centro(r.center().x, r.center().y + 5.0 * f, nome, 15, if i == self.aba { estilo::OURO } else { estilo::TEXTO });
            if clicou_livre && r.contains(m) {
                self.aba = i;
            }
            x += tw + 8.0 * f;
        }

        // Cartoes.
        let topo = ya + 50.0 * f;
        let rodape = 70.0 * f;
        let vao = 12.0 * f;
        let cw = (p.w - 40.0 * f - vao * (COLUNAS as f32 - 1.0)) / COLUNAS as f32;
        let cartoes: Vec<Cartao> = match self.aba {
            0 => cat::MONTARIAS.iter().map(|mt| cartao_de_montaria(mt, &estado)).collect(),
            1 => cat::SKINS.iter().filter(|s| s.preco_tp > 0).map(|s| cartao_de_skin(s, &estado)).collect(),
            _ => cat::PACOTES.iter().map(cartao_de_pacote).collect(),
        };
        let linhas = cartoes.len().div_ceil(COLUNAS).max(1);
        let ch = ((p.y + p.h - rodape - topo - vao * (linhas as f32 - 1.0)) / linhas as f32).min(250.0 * f);
        for (i, c) in cartoes.iter().enumerate() {
            let r = Rect::new(x0 + (i % COLUNAS) as f32 * (cw + vao), topo + (i / COLUNAS) as f32 * (ch + vao), cw, ch);
            estilo::cartao(r, r.contains(m), c.possui);
            let raio = (ch * 0.2).min(cw * 0.16);
            medalhao(vec2(r.x + 14.0 * f + raio, r.y + 14.0 * f + raio), raio, c.cor, &c.letra);
            let tx = r.x + 28.0 * f + raio * 2.0;
            estilo::texto_ajustado(&c.titulo, tx, r.y + 30.0 * f, r.x + r.w - tx - 10.0 * f, 16, estilo::TEXTO);
            estilo::texto_ajustado(&c.sub, tx, r.y + 52.0 * f, r.x + r.w - tx - 10.0 * f, 12, estilo::SUAVE);
            if !c.linha.is_empty() {
                estilo::texto_ajustado(&c.linha, r.x + 14.0 * f, r.y + raio * 2.0 + 44.0 * f, r.w - 28.0 * f, 12, estilo::SUAVE);
            }
            let bot = Rect::new(r.x + 12.0 * f, r.y + r.h - 50.0 * f, r.w - 24.0 * f, 40.0 * f);
            let ativo = c.acao.is_some() && !self.em_voo;
            estilo::cartao(bot, ativo && bot.contains(m), ativo);
            estilo::texto_centro_forte(bot.center().x, bot.center().y + 6.0 * f, &c.botao, 15, if ativo { estilo::OURO } else { estilo::SUAVE });
            if ativo && clicou_livre && bot.contains(m) {
                self.confirma = c.acao;
            }
        }

        // Rodape: ultima compra e historico curto.
        let yr = p.y + p.h - rodape + 18.0 * f;
        if let Some((ok, t)) = &self.ultimo {
            let cor = if *ok { Color::new(0.45, 0.85, 0.52, 1.0) } else { Color::new(0.95, 0.45, 0.4, 1.0) };
            estilo::texto_ajustado(t, x0, yr + 8.0 * f, p.w * 0.5, 14, cor);
        }
        let hist: Vec<String> = estado.historico.iter().take(3).map(|c| format!("{} · {} · {}", c.produto, c.valor, c.status)).collect();
        for (i, l) in hist.iter().enumerate() {
            estilo::texto_ajustado(l, p.x + p.w * 0.52, yr - 4.0 * f + i as f32 * 17.0 * f, p.w * 0.46, 11, estilo::SUAVE);
        }

        // Confirmacao por cima de tudo.
        if let Some(conf) = self.confirma {
            let (titulo, preco) = match conf {
                Confirma::Tp(id) => {
                    let pk = cat::pacote(id).copied().unwrap_or(cat::PACOTES[0]);
                    (format!("{} ({} TP)", pk.nome, milhar(pk.total())), cat::preco_brl(pk.centavos))
                }
                Confirma::Item(pr) => (pr.nome(), format!("{} TP", milhar(pr.preco_tp().unwrap_or(0)))),
            };
            let cwid = (480.0 * f).min(p.w - 40.0 * f);
            let r = Rect::new(p.center().x - cwid * 0.5, p.center().y - 110.0 * f, cwid, 220.0 * f);
            crate::hud_layout::escurece(0.35);
            estilo::painel(r);
            estilo::texto_centro_forte(r.center().x, r.y + 40.0 * f, "Confirmar compra", 18, estilo::OURO);
            estilo::texto_centro(r.center().x, r.y + 76.0 * f, &titulo, 16, estilo::TEXTO);
            estilo::texto_centro_forte(r.center().x, r.y + 104.0 * f, &preco, 18, estilo::TEXTO);
            if matches!(conf, Confirma::Tp(_)) && estado.simulado {
                estilo::texto_centro(r.center().x, r.y + 128.0 * f, "Pagamento simulado: nada é cobrado.", 12, estilo::SUAVE);
            }
            let bw = (r.w - 60.0 * f) * 0.5;
            let sim = Rect::new(r.x + 20.0 * f, r.y + r.h - 62.0 * f, bw, 44.0 * f);
            let nao = Rect::new(r.x + r.w - 20.0 * f - bw, sim.y, bw, 44.0 * f);
            estilo::cartao(sim, sim.contains(m), true);
            estilo::texto_centro_forte(sim.center().x, sim.center().y + 6.0 * f, "Comprar", 16, estilo::OURO);
            estilo::cartao(nao, nao.contains(m), false);
            estilo::texto_centro(nao.center().x, nao.center().y + 6.0 * f, "Cancelar", 16, estilo::TEXTO);
            if is_mouse_button_pressed(MouseButton::Left) {
                if sim.contains(m) {
                    let pedido = self.novo_pedido();
                    saida.push(ClientMessage::Loja {
                        pedido: match conf {
                            Confirma::Tp(pacote) => PedidoLoja::ComprarTp { pacote, pedido },
                            Confirma::Item(produto) => PedidoLoja::ComprarItem { produto, pedido },
                        },
                    });
                    self.em_voo = true;
                    self.confirma = None;
                } else if nao.contains(m) || !r.contains(m) {
                    self.confirma = None;
                }
            }
        }
        saida
    }
}

struct Cartao {
    titulo: String,
    sub: String,
    linha: String,
    cor: Color,
    letra: String,
    botao: String,
    acao: Option<Confirma>,
    possui: bool,
}

fn cartao_de_montaria(mt: &cat::Montaria, e: &EstadoLoja) -> Cartao {
    let possui = e.posses.montarias.contains(&mt.id);
    Cartao {
        titulo: mt.nome.to_string(),
        sub: format!("Velocidade montado +{:.0}%", (cat::VEL_MONTADO - 1.0) * 100.0),
        linha: mt.descricao.to_string(),
        cor: cor_da_montaria(mt.id),
        letra: mt.nome.chars().next().unwrap_or('?').to_string(),
        botao: if possui { "Possui".into() } else { format!("{} TP", milhar(mt.preco_tp)) },
        acao: (!possui).then_some(Confirma::Item(Produto::Montaria(mt.id))),
        possui,
    }
}

fn cartao_de_skin(s: &cat::Skin, e: &EstadoLoja) -> Cartao {
    let possui = e.posses.skins.contains(&s.id);
    let tem_montaria = e.posses.montarias.contains(&s.montaria);
    let nome_m = cat::montaria(s.montaria).map_or("?", |m| m.nome);
    Cartao {
        titulo: s.nome.to_string(),
        sub: format!("Skin de {nome_m}"),
        linha: if tem_montaria || possui { "Só aparência: não muda velocidade nem combate.".into() } else { "Compre a montaria antes.".into() },
        cor: cor_da_skin(s.id),
        letra: nome_m.chars().next().unwrap_or('?').to_string(),
        botao: if possui { "Possui".into() } else { format!("{} TP", milhar(s.preco_tp)) },
        acao: (!possui && tem_montaria).then_some(Confirma::Item(Produto::Skin(s.id))),
        possui,
    }
}

fn cartao_de_pacote(pk: &cat::PacoteTp) -> Cartao {
    Cartao {
        titulo: format!("{} TP", milhar(pk.total())),
        sub: pk.nome.to_string(),
        linha: if pk.bonus > 0 { format!("{} + {} de bônus", milhar(pk.tp), milhar(pk.bonus)) } else { String::new() },
        cor: Color::from_rgba(120, 180, 255, 255),
        letra: "TP".into(),
        botao: cat::preco_brl(pk.centavos),
        acao: Some(Confirma::Tp(pk.id)),
        possui: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pedidos_novos_sao_validos_e_diferentes() {
        let mut l = LojaTp::default();
        let a = l.novo_pedido();
        let b = l.novo_pedido();
        assert!(cat::pedido_valido(&a), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn cartoes_respeitam_posses() {
        let e = EstadoLoja { ligada: true, posses: cat::Posses { montarias: vec![1], skins: vec![101] }, ..Default::default() };
        let lobo = cartao_de_montaria(&cat::MONTARIAS[0], &e);
        assert!(lobo.possui && lobo.acao.is_none());
        let tigre = cartao_de_montaria(&cat::MONTARIAS[1], &e);
        assert_eq!(tigre.acao, Some(Confirma::Item(Produto::Montaria(2))));
        let skin_lobo = cartao_de_skin(cat::skin(102).unwrap(), &e);
        assert!(skin_lobo.acao.is_some());
        let skin_tigre = cartao_de_skin(cat::skin(202).unwrap(), &e);
        assert!(skin_tigre.acao.is_none(), "sem a montaria nao compra a skin");
    }

    #[test]
    fn resultado_vai_pro_chat_e_libera_o_botao() {
        let mut l = LojaTp { em_voo: true, ..Default::default() };
        assert_eq!(l.receber(AvisoLoja::Resultado { ok: true, texto: "ok".into() }), Some("Loja: ok".into()));
        assert!(!l.em_voo);
    }
}
