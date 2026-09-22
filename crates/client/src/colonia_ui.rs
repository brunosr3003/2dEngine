//! O painel da COLONIA (docs/COLONIA.md).
//!
//! Tres eixos pra melhorar, uma colheita pra recolher e a saida pro porto.
//! Quem decide tudo e' o servidor (`shared::colonia`): aqui so' se desenha o
//! estado que ele mandou e se devolve o pedido que o dedo encostou.

use macroquad::prelude::*;
use shared::colonia::{eixo, PedidoColonia, EIXOS, NIVEL_MAX};

use crate::hud_estilo as estilo;

/// O estado que o servidor mandou, do jeito que ele mandou.
#[derive(Debug, Clone)]
pub struct Estado {
    pub niveis: [u8; EIXOS],
    pub horas: f32,
    pub colheita: Vec<(u16, u32)>,
    pub custos: Vec<Vec<(u16, u32)>>,
    pub banco: u8,
    /// O que esta' NO BAU da ilha. A colheita cai aqui.
    pub bau: Vec<shared::InventorySlot>,
    /// Os moradores, na ordem das vagas.
    pub trabalhadores: Vec<shared::colonia::Profissao>,
    /// Quantas vagas o assentamento sustenta. Maior que `trabalhadores.len()`
    /// = ha' casa vazia esperando alguem.
    pub vagas: u8,
}

#[derive(Debug, Default)]
pub struct ColoniaUi {
    estado: Option<Estado>,
}

impl ColoniaUi {
    pub fn abrir(&mut self, e: Estado) {
        self.estado = Some(e);
    }

    /// Atualiza SEM abrir: colher e melhorar mandam estado novo, e se isso
    /// abrisse o painel ele reapareceria sozinho no porto.
    pub fn atualizar(&mut self, e: Estado) {
        if self.estado.is_some() {
            self.estado = Some(e);
        }
    }

    pub fn fechar(&mut self) {
        self.estado = None;
    }

    pub fn aberto(&self) -> bool {
        self.estado.is_some()
    }

    /// Desenha; devolve o pedido que o jogador fez.
    pub fn desenha(&mut self, nome_item: &dyn Fn(u16) -> String) -> Option<PedidoColonia> {
        estilo::no_painel(estilo::escala_do_painel(600.0, 520.0), || {
            self.desenha_na_escala(nome_item)
        })
    }

    fn desenha_na_escala(&mut self, nome_item: &dyn Fn(u16) -> String) -> Option<PedidoColonia> {
        let e = self.estado.as_ref()?.clone();
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha_h = 86.0 * f;
        let w = (600.0 * f).min(seguro.w - 16.0);
        // A altura conta as VAGAS: sem isso o rodape cobria os moradores, que
        // foi o mesmo defeito do painel de criacao de personagem.
        let vagas_h = if e.vagas > 0 {
            32.0 * f + 48.0 * f * e.vagas as f32
        } else {
            0.0
        };
        let h = (216.0 * f + 64.0 * f + linha_h * EIXOS as f32 + vagas_h).min(seguro.h - 16.0);
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
        estilo::texto_forte(x0, p.y + 36.0 * f, "Minha Ilha", 20, estilo::OURO);
        estilo::texto(x0, p.y + 60.0 * f, &resumo(&e), 14, estilo::SUAVE);

        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );

        let mut pedido = None;

        // COLHEITA no topo: e' o motivo de abrir o painel.
        let cr = Rect::new(x0, p.y + 76.0 * f, p.w - 40.0 * f, 56.0 * f);
        estilo::cartao(cr, false, !e.colheita.is_empty());
        estilo::texto(
            cr.x + 14.0 * f,
            cr.y + 24.0 * f,
            &colheita_em_texto(&e, nome_item),
            14,
            if e.colheita.is_empty() {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            },
        );
        estilo::texto(
            cr.x + 14.0 * f,
            cr.y + 44.0 * f,
            &aviso_do_relogio(e.horas),
            12,
            cor_do_relogio(e.horas),
        );
        if !e.colheita.is_empty() {
            let b = Rect::new(
                cr.x + cr.w - 136.0 * f,
                cr.y + (cr.h - 40.0 * f) * 0.5,
                124.0 * f,
                40.0 * f,
            );
            estilo::botao(b, "Colher", estilo::estado_de(b, false, false), true);
            if clicou && b.contains(m) {
                pedido = Some(PedidoColonia::Colher);
            }
        }

        // O BAU DA ILHA, logo abaixo da colheita: e' onde ela cai, e nao na
        // bolsa. Sem esta linha o jogador colhia e nao achava nada.
        let mut y = cr.y + cr.h + 12.0 * f;
        {
            let br = Rect::new(x0, y, p.w - 40.0 * f, 52.0 * f);
            estilo::cartao(br, false, !e.bau.is_empty());
            let dentro: Vec<(u16, u32)> = e.bau.iter().map(|s| (s.item_id, s.qty)).collect();
            estilo::texto_forte(
                br.x + 14.0 * f,
                br.y + 22.0 * f,
                &format!("Baú da ilha · {}/{} espaços", e.bau.len(), e.banco),
                14,
                estilo::OURO,
            );
            estilo::texto(
                br.x + 14.0 * f,
                br.y + 42.0 * f,
                &if dentro.is_empty() {
                    "Vazio — a colheita cai aqui.".to_string()
                } else {
                    lista(&dentro, nome_item)
                },
                12,
                if dentro.is_empty() { estilo::SUAVE } else { estilo::TEXTO },
            );
            if !e.bau.is_empty() {
                let b = Rect::new(
                    br.x + br.w - 136.0 * f,
                    br.y + (br.h - 38.0 * f) * 0.5,
                    124.0 * f,
                    38.0 * f,
                );
                estilo::botao(b, "Retirar", estilo::estado_de(b, false, false), false);
                if clicou && b.contains(m) {
                    pedido = Some(PedidoColonia::Retirar);
                }
            }
            y += br.h + 12.0 * f;
        }

        // Os tres eixos.
        for i in 0..EIXOS {
            let r = Rect::new(x0, y, p.w - 40.0 * f, linha_h - 10.0 * f);
            let no_maximo = e.niveis[i] >= NIVEL_MAX;
            estilo::cartao(r, false, false);
            estilo::texto_forte(
                r.x + 14.0 * f,
                r.y + 24.0 * f,
                &format!("{} {}/{}", eixo::NOMES[i], e.niveis[i], NIVEL_MAX),
                16,
                estilo::TEXTO,
            );
            estilo::texto(r.x + 14.0 * f, r.y + 46.0 * f, &efeito(i, &e), 12, estilo::SUAVE);
            let custo = e.custos.get(i).cloned().unwrap_or_default();
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 66.0 * f,
                &if no_maximo {
                    "No máximo.".to_string()
                } else {
                    format!("Custa {}", lista(&custo, nome_item))
                },
                12,
                estilo::SUAVE,
            );
            if !no_maximo {
                let b = Rect::new(
                    r.x + r.w - 136.0 * f,
                    r.y + (r.h - 40.0 * f) * 0.5,
                    124.0 * f,
                    40.0 * f,
                );
                estilo::botao(b, "Melhorar", estilo::estado_de(b, false, false), false);
                if clicou && b.contains(m) {
                    pedido = Some(PedidoColonia::Melhorar { eixo: i as u8 });
                }
            }
            y += linha_h;
        }

        // OS MORADORES. Uma linha por vaga: quem esta' nela, ou o convite
        // pra alguem. Vem DEPOIS dos eixos porque o assentamento e' quem abre
        // as vagas — a ordem da tela e' a ordem em que se faz a coisa.
        if e.vagas > 0 {
            estilo::texto_forte(x0, y + 20.0 * f, "Moradores", 16, estilo::OURO);
            y += 32.0 * f;
            for vaga in 0..e.vagas as usize {
                let r = Rect::new(x0, y, p.w - 40.0 * f, 44.0 * f);
                let quem = e.trabalhadores.get(vaga).copied();
                estilo::cartao(r, false, quem.is_some());
                let texto = match quem {
                    Some(q) => {
                        let (item, qtd) = shared::colonia::por_hora_do_trabalhador(
                            q,
                            e.niveis[eixo::RECURSOS],
                        );
                        format!("{} · {} {}/h", q.nome(), qtd, nome_item(item))
                    }
                    None => "Casa vazia — escolha um ofício".to_string(),
                };
                estilo::texto(
                    r.x + 12.0 * f,
                    r.y + 27.0 * f,
                    &texto,
                    13,
                    if quem.is_some() { estilo::TEXTO } else { estilo::SUAVE },
                );
                // Um botao por oficio, do lado direito. Sao cinco e cabem:
                // uma lista suspensa esconderia a escolha atras de um toque.
                let bw = 62.0 * f;
                let total = shared::colonia::Profissao::TODAS.len() as f32;
                let mut bx = r.x + r.w - bw * total - 8.0 * f;
                for op in shared::colonia::Profissao::TODAS {
                    let b = Rect::new(bx, r.y + 6.0 * f, bw - 4.0 * f, 32.0 * f);
                    let posto = quem == Some(op);
                    estilo::botao(
                        b,
                        &op.nome()[..3.min(op.nome().len())],
                        estilo::estado_de(b, posto, false),
                        posto,
                    );
                    if clicou && b.contains(m) {
                        pedido = Some(if posto {
                            PedidoColonia::Demitir { vaga: vaga as u8 }
                        } else {
                            PedidoColonia::Contratar {
                                vaga: vaga as u8,
                                oficio: op.indice(),
                            }
                        });
                    }
                    bx += bw;
                }
                y += 48.0 * f;
            }
        }

        // A saida. Fica no rodape e nao entre os eixos: sair e' o unico
        // botao daqui que nao se desfaz.
        let volta = Rect::new(x0, p.y + p.h - 56.0 * f, 200.0 * f, 42.0 * f);
        estilo::botao(
            volta,
            "Voltar ao porto",
            estilo::estado_de(volta, false, false),
            false,
        );
        if clicou && volta.contains(m) {
            pedido = Some(PedidoColonia::Voltar);
        }

        // Voltar fecha o painel. Colher e melhorar NAO: o jogador quase sempre
        // faz os dois seguidos, e fechar a cada toque custa um toque a mais.
        if matches!(pedido, Some(PedidoColonia::Voltar))
            || (clicou && (fechar.contains(m) || !p.contains(m)))
        {
            self.fechar();
        }
        pedido
    }
}

fn resumo(e: &Estado) -> String {
    format!(
        "Tamanho {} · Recursos {} · Banco {} espaços",
        e.niveis[eixo::TAMANHO],
        e.niveis[eixo::RECURSOS],
        e.banco
    )
}

fn efeito(i: usize, e: &Estado) -> String {
    let n = e.niveis[i];
    match i {
        eixo::TAMANHO => format!("Ilha de {} blocos de raio.", shared::colonia::raio_blocos(n)),
        eixo::RECURSOS => "Rende mais por hora, mesmo com você fora.".into(),
        _ => format!("{} espaços no banco daqui.", shared::colonia::espacos_do_banco(n)),
    }
}

fn lista(itens: &[(u16, u32)], nome_item: &dyn Fn(u16) -> String) -> String {
    if itens.is_empty() {
        return "nada".into();
    }
    itens
        .iter()
        .map(|(id, q)| format!("{q}× {}", nome_item(*id)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn colheita_em_texto(e: &Estado, nome_item: &dyn Fn(u16) -> String) -> String {
    if e.colheita.is_empty() {
        "A ilha ainda não rendeu nada.".into()
    } else {
        format!("Pronto para colher: {}", lista(&e.colheita, nome_item))
    }
}

/// O relogio e' a regra inteira da colonia numa linha, e por isso ele diz o
/// que esta' acontecendo AGORA e nao so' quantas horas passaram: e' a
/// diferenca entre "12h" e "passou das 12h, e desde entao rende metade".
fn aviso_do_relogio(horas: f32) -> String {
    let h = shared::colonia::HORAS_CHEIAS;
    let teto = shared::colonia::HORAS_TETO;
    if horas < h {
        format!("{horas:.0}h de {h:.0}h — rendendo cheio.")
    } else if horas < teto {
        format!("{horas:.0}h — passou das {h:.0}h e desde então rende metade.")
    } else {
        format!("{horas:.0}h — parou de render às {teto:.0}h. Colha.")
    }
}

fn cor_do_relogio(horas: f32) -> Color {
    if horas >= shared::colonia::HORAS_TETO {
        estilo::VERMELHO
    } else if horas >= shared::colonia::HORAS_CHEIAS {
        estilo::OURO
    } else {
        estilo::VERDE
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn e(horas: f32) -> Estado {
        Estado {
            niveis: [1; EIXOS],
            horas,
            colheita: vec![],
            custos: vec![],
            banco: 10,
            bau: vec![],
            trabalhadores: vec![],
            vagas: 0,
        }
    }

    /// As tres fases da regra tem que ser DISTINGUIVEIS na tela. Se as duas
    /// primeiras dissessem a mesma coisa, o jogador nao teria como aprender
    /// que voltar antes das 12h rende mais — que e' a unica decisao que a
    /// colonia pede dele.
    #[test]
    fn o_relogio_conta_as_tres_fases() {
        let cheio = aviso_do_relogio(6.0);
        let meio = aviso_do_relogio(18.0);
        let parado = aviso_do_relogio(30.0);
        assert!(cheio.contains("cheio"), "{cheio}");
        assert!(meio.contains("metade"), "{meio}");
        assert!(parado.contains("Colha"), "{parado}");
        assert_ne!(cor_do_relogio(6.0), cor_do_relogio(18.0));
        assert_ne!(cor_do_relogio(18.0), cor_do_relogio(30.0));
    }
}
