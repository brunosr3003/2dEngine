//! THE PATH TREE screen ("árvore de caminhos", `shared::arvore`, rebirth
//! phase 2): four columns (Assassin, Mage, Guardian, Corsair) of 15 nodes,
//! the selected node's description, the free points, "Learn" and the reset
//! for gold (with confirmation). Opens from the Character Sheet's PATHS
//! button (after the first rebirth). The screen only shows what the server
//! sent (`ServerMessage::Arvore`) and sends requests; the server checks the
//! budget, the order and the gold.

#[allow(unused_imports)]
use crate::ui::Toque as _;
use macroquad::prelude::*;
use shared::arvore::{self as av, Caminho, EstadoArvore, PedidoArvore, NOS_POR_CAMINHO};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

const LARGURA: f32 = 900.0;
const ALTURA: f32 = 580.0;
/// Nodes per row inside a path's column (15 = 5 rows of 3).
const POR_LINHA: usize = 3;

#[derive(Default)]
pub struct ArvoreUi {
    pub aberta: bool,
    /// The server's last state; `None` until it arrives.
    pub estado: Option<EstadoArvore>,
    /// The tapped node: (path, node 0-based).
    selecionado: (u8, u8),
    confirmar_redistribuir: bool,
}

impl ArvoreUi {
    /// Opens on the first path that still has a node to learn.
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.confirmar_redistribuir = false;
        if let Some(e) = &self.estado {
            if let Some(c) = (0..av::CAMINHOS).find(|&c| e.arvore.nos[c] < NOS_POR_CAMINHO) {
                self.selecionado = (c as u8, e.arvore.nos[c]);
            }
        }
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.confirmar_redistribuir = false;
    }

    pub fn receber(&mut self, mut estado: EstadoArvore) {
        // The slots travel beside the allocation (`Arvore::slots` is not on
        // the wire): keep both views the same.
        estado.arvore.slots = estado.slots;
        self.estado = Some(estado);
    }

    /// Free points to spend: the red dot on the Sheet (and the Menu).
    pub fn tem_ponto_livre(&self) -> bool {
        self.estado.is_some_and(|e| e.livres() > 0 && e.arvore.gastos() < 60)
    }

    /// Can node `k` of path `c` be learned right now (the next one, with a
    /// free point)? The same rule as `Arvore::gastar` on the server.
    fn pode_aprender(&self, c: u8, k: u8) -> bool {
        self.estado.is_some_and(|e| {
            (c as usize) < av::CAMINHOS && e.arvore.nos[c as usize] == k && k < NOS_POR_CAMINHO && e.livres() > 0
        })
    }

    pub fn desenha(&mut self, ouro: u64) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        let seguro = crate::hud_layout::tela_segura();
        let escala = estilo::escala_do_painel(LARGURA, ALTURA)
            .min((seguro.w - 16.0) / LARGURA)
            .min((seguro.h - 16.0) / ALTURA);
        estilo::no_painel(escala, || {
            let modal = self.confirmar_redistribuir;
            let pedido = self.desenha_painel(ouro, !modal);
            if modal {
                return self.desenha_confirmacao(ouro);
            }
            pedido
        })
    }

    fn desenha_painel(&mut self, ouro: u64, ativo: bool) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (LARGURA * f).min(seguro.w - 16.0);
        let h = (ALTURA * f).min(seguro.h - 16.0);
        let p = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = crate::foco::clique() && ativo;

        estilo::texto_forte(p.x + 20.0 * f, p.y + 36.0 * f, "PATHS", 23, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.toca(m) {
            self.fechar();
            return None;
        }
        let Some(estado) = self.estado else {
            estilo::texto(p.x + 20.0 * f, p.y + 80.0 * f, "Waiting for the server…", 15, estilo::SUAVE);
            return None;
        };
        let livres = estado.livres();
        estilo::texto(
            p.x + 120.0 * f,
            p.y + 35.0 * f,
            &format!("Free points: {} of {}", livres, estado.ganhos),
            16,
            if livres > 0 { estilo::VERDE } else { estilo::SUAVE },
        );
        let regra = "1 point every 10 levels, from the first rebirth on";
        estilo::texto_ajustado(regra, p.x + 20.0 * f, p.y + 62.0 * f, p.w - 120.0 * f, 13, estilo::SUAVE);

        // ── the four columns ──
        let topo = p.y + 76.0 * f;
        let col_w = (p.w - 40.0 * f - 3.0 * 10.0 * f) / 4.0;
        let lado = ((col_w - 16.0 * f - (POR_LINHA as f32 - 1.0) * 6.0 * f) / POR_LINHA as f32).min(52.0 * f);
        let mut pedido = None;
        for c in Caminho::TODOS {
            let ci = c as usize;
            let col = Rect::new(p.x + 20.0 * f + ci as f32 * (col_w + 10.0 * f), topo, col_w, 370.0 * f);
            estilo::cartao(col, false, self.selecionado.0 as usize == ci);
            let cor = cor_do_caminho(c);
            estilo::texto_forte(col.x + 10.0 * f, col.y + 24.0 * f, c.nome(), 17, cor);
            let feitos = estado.arvore.nos[ci];
            let contagem = format!("{feitos}/{NOS_POR_CAMINHO}");
            estilo::texto(
                col.x + col.w - 10.0 * f - estilo::medir(&contagem, 14),
                col.y + 24.0 * f,
                &contagem,
                14,
                if feitos >= NOS_POR_CAMINHO { estilo::OURO } else { estilo::SUAVE },
            );
            estilo::texto_ajustado(c.resumo(), col.x + 10.0 * f, col.y + 44.0 * f, col.w - 20.0 * f, 11, estilo::SUAVE);
            let largura_grade = POR_LINHA as f32 * lado + (POR_LINHA as f32 - 1.0) * 6.0 * f;
            let x0 = col.x + (col.w - largura_grade) * 0.5;
            for k in 0..NOS_POR_CAMINHO {
                let (lin, coluna) = (k as usize / POR_LINHA, k as usize % POR_LINHA);
                let r = Rect::new(
                    x0 + coluna as f32 * (lado + 6.0 * f),
                    col.y + 58.0 * f + lin as f32 * (lado + 9.0 * f),
                    lado,
                    lado,
                );
                let no = av::no(c, k).expect("15 nodes");
                let tem = estado.arvore.tem(c, k);
                let proximo = feitos == k;
                let escolhido = self.selecionado == (ci as u8, k);
                estilo::cartao(r, r.toca(m), escolhido);
                if tem {
                    estilo::ret_arredondado(
                        Rect::new(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h - 6.0),
                        4.0,
                        estilo::alfa(cor, 0.42),
                    );
                } else if proximo && livres > 0 {
                    estilo::borda_arredondada(r, 4.0, 2.0, estilo::alfa(estilo::VERDE, 0.85));
                }
                let cor_txt = if tem { estilo::TEXTO } else if proximo { estilo::ACENTO } else { estilo::SUAVE };
                estilo::texto_centro_forte(r.center().x, r.y + r.h * 0.44, &(k + 1).to_string(), 15, cor_txt);
                let valor = format!("+{}", no.efeito.numero(no.valor));
                estilo::texto_centro(r.center().x, r.y + r.h * 0.82, &valor, 10, cor_txt);
                if no.ativa.is_some() {
                    // The active node's mark: a small gold diamond in the corner.
                    let d = vec2(r.x + r.w - 8.0 * f, r.y + 8.0 * f);
                    let s = 4.5 * f;
                    draw_triangle(d + vec2(0.0, -s), d + vec2(s, 0.0), d + vec2(0.0, s), estilo::OURO);
                    draw_triangle(d + vec2(0.0, -s), d + vec2(-s, 0.0), d + vec2(0.0, s), estilo::OURO);
                }
                if clique && r.toca(m) {
                    self.selecionado = (ci as u8, k);
                }
            }
        }

        // ── the selected node ──
        let (sc, sk) = self.selecionado;
        let c = Caminho::de_indice(sc).unwrap_or(Caminho::Assassino);
        let no = av::no(c, sk).unwrap_or(&av::NOS[0][0]);
        let info = Rect::new(p.x + 20.0 * f, topo + 380.0 * f, p.w - 40.0 * f, p.h - (topo - p.y) - 390.0 * f);
        let x = info.x + 4.0 * f;
        // The text column stops before the buttons.
        let larg = info.w - 250.0 * f;
        estilo::texto_forte(
            x,
            info.y + 18.0 * f,
            &format!("{} · node {}/{}", c.nome(), sk + 1, NOS_POR_CAMINHO),
            16,
            cor_do_caminho(c),
        );
        estilo::texto_ajustado(&linha_do_no(no), x, info.y + 42.0 * f, larg, 17, estilo::TEXTO);
        if let Some(id) = no.ativa {
            let nome = shared::idioma::tr(av::nome_da_ativa(id)).into_owned();
            let d = av::def(id).expect("a known active");
            let linha = if av::e_suprema(id, estado.renascimentos) {
                format!("Active: {} (Supreme) · {} MP · {}s cooldown", nome, d.custo_mp, d.espera_s as u32)
            } else {
                format!("Active: {} · {} MP · {}s cooldown", nome, d.custo_mp, d.espera_s as u32)
            };
            estilo::texto_ajustado(&linha, x, info.y + 64.0 * f, larg, 13, estilo::OURO);
        }
        let tem = estado.arvore.tem(c, sk);
        let feitos = estado.arvore.nos[sc as usize];
        let ativa_aprendida = no.ativa.filter(|_| tem);
        let (situacao, cor_sit) = if let Some(id) = ativa_aprendida {
            match estado.slots.iter().position(|s| *s == Some(id)) {
                Some(i) => (format!("Learned · in skill slot {}", i + 4), estilo::VERDE),
                None if estado.slots_abertos == 0 => {
                    ("Learned · skill slot 4 opens at the first rebirth".to_string(), estilo::SUAVE)
                }
                None => ("Learned · equip it in a skill slot: 4, 5 or 6".to_string(), estilo::ACENTO),
            }
        } else if tem {
            ("Learned".to_string(), estilo::VERDE)
        } else if feitos == sk && livres > 0 {
            ("Ready to learn: 1 point".to_string(), estilo::ACENTO)
        } else if feitos == sk {
            ("No free path points.".to_string(), estilo::VERMELHO)
        } else {
            (format!("Learn node {} of this path first.", sk), estilo::SUAVE)
        };
        estilo::texto_ajustado(&situacao, x, info.y + 86.0 * f, larg, 13, cor_sit);
        let total: Vec<String> =
            linhas_do_caminho(c, feitos).iter().map(|l| shared::idioma::tr(l).into_owned()).collect();
        // An active node shows what the active does instead of the path sum.
        if let Some(d) = no.ativa.and_then(av::def) {
            estilo::texto_ajustado(d.descricao, x, info.y + 106.0 * f, larg, 11, estilo::SUAVE);
        } else if !total.is_empty() {
            estilo::texto_ajustado(
                &format!("This path so far: {}", total.join(" · ")),
                x,
                info.y + 106.0 * f,
                larg,
                11,
                estilo::SUAVE,
            );
        }

        // ── buttons ──
        let bh = 42.0 * f;
        let aprender = Rect::new(info.x + info.w - 230.0 * f, info.y + 6.0 * f, 230.0 * f, bh);
        let pode = self.pode_aprender(sc, sk);
        if let Some(id) = ativa_aprendida {
            // A learned active: three slot buttons in the Learn button's place.
            // Tap a free one to equip, the one holding it to take it off.
            let gap = 8.0 * f;
            let bw = (aprender.w - 2.0 * gap) / 3.0;
            for i in 0..av::SLOTS_EXTRAS {
                let r = Rect::new(aprender.x + i as f32 * (bw + gap), aprender.y, bw, bh);
                let aberto = i < estado.slots_abertos as usize;
                let aqui = estado.slots[i] == Some(id);
                let rotulo = if aberto {
                    (i + 4).to_string()
                } else {
                    format!("R{}", av::ABRE_SLOT_EM[i])
                };
                estilo::botao(r, &rotulo, estilo::estado_de(r, !aberto, aqui), aqui);
                if clique && aberto && r.toca(m) {
                    let ativa = if aqui { None } else { Some(id) };
                    pedido = Some(ClientMessage::Arvore {
                        pedido: PedidoArvore::Equipar { slot: i as u8, ativa },
                    });
                }
            }
        } else {
            estilo::botao(aprender, "Learn", estilo::estado_de(aprender, !pode, false), pode);
        }
        if clique && aprender.toca(m) && pode && ativa_aprendida.is_none() {
            pedido = Some(ClientMessage::Arvore { pedido: PedidoArvore::Gastar { caminho: sc, no: sk } });
            // Walk to the next node, so a second tap learns the next one.
            if sk + 1 < NOS_POR_CAMINHO {
                self.selecionado = (sc, sk + 1);
            }
        }
        let gastos = estado.arvore.gastos();
        let reset = Rect::new(aprender.x, aprender.y + bh + 10.0 * f, aprender.w, 34.0 * f);
        let rotulo = format!("Reset paths · {} gold", crate::bolsa::milhar(estado.custo_redistribuir));
        let pode_reset = gastos > 0;
        estilo::botao(reset, &rotulo, estilo::estado_de(reset, !pode_reset, false), false);
        if clique && reset.toca(m) && pode_reset {
            self.confirmar_redistribuir = true;
        }
        let saldo = format!("Gold: {}", crate::bolsa::milhar(ouro));
        estilo::texto(
            reset.x + reset.w - estilo::medir(&saldo, 12),
            reset.y + reset.h + 16.0 * f,
            &saldo,
            12,
            if ouro >= estado.custo_redistribuir { estilo::SUAVE } else { estilo::VERMELHO },
        );
        pedido
    }

    /// "Reset all paths?": the points come back, the gold goes.
    fn desenha_confirmacao(&mut self, ouro: u64) -> Option<ClientMessage> {
        let estado = self.estado?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = ((600.0 * f).min(seguro.w - 24.0), (250.0 * f).min(seguro.h - 24.0));
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        crate::hud_layout::escurece(0.5);
        estilo::painel(r);
        let x = r.x + 24.0 * f;
        let larg = w - 48.0 * f;
        let custo = estado.custo_redistribuir;
        estilo::texto_forte(x, r.y + 40.0 * f, "Reset all paths?", 22, estilo::OURO);
        estilo::texto_ajustado(
            &format!(
                "All {} points come back to spend again, for {} gold.",
                estado.arvore.gastos(),
                crate::bolsa::milhar(custo)
            ),
            x,
            r.y + 82.0 * f,
            larg,
            15,
            estilo::TEXTO,
        );
        let paga = ouro >= custo;
        estilo::texto(
            x,
            r.y + 114.0 * f,
            &format!("You have {} gold.", crate::bolsa::milhar(ouro)),
            14,
            if paga { estilo::SUAVE } else { estilo::VERMELHO },
        );
        let bw = (w - 60.0 * f) * 0.5;
        let bh = 46.0 * f;
        let by = r.y + r.h - 22.0 * f - bh;
        let cancelar = Rect::new(x, by, bw, bh);
        let ok = Rect::new(cancelar.x + bw + 12.0 * f, by, bw, bh);
        estilo::botao(cancelar, "Cancel", estilo::estado_de(cancelar, false, false), false);
        estilo::botao(ok, "Reset", estilo::estado_de(ok, !paga, false), paga);
        if !crate::foco::clique() {
            return None;
        }
        let m = Vec2::from(mouse_position());
        if ok.toca(m) && paga {
            self.confirmar_redistribuir = false;
            return Some(ClientMessage::Arvore { pedido: PedidoArvore::Redistribuir });
        }
        if cancelar.toca(m) || !r.toca(m) {
            self.confirmar_redistribuir = false;
        }
        None
    }
}

fn cor_do_caminho(c: Caminho) -> Color {
    match c {
        Caminho::Assassino => Color::new(0.95, 0.42, 0.40, 1.0),
        Caminho::Mago => Color::new(0.52, 0.66, 1.0, 1.0),
        Caminho::Guardiao => Color::new(0.93, 0.78, 0.42, 1.0),
        Caminho::Corsario => Color::new(0.36, 0.86, 0.78, 1.0),
    }
}

/// "Crit chance +1%": the English template with the number in its hole (the
/// text helpers translate the whole line by the template).
fn linha_do_no(no: &av::No) -> String {
    no.efeito.modelo().replace("{}", &no.efeito.numero(no.valor))
}

/// The first `feitos` nodes of path `c`, summed per effect, as English
/// lines (translate each one to show it).
fn linhas_do_caminho(c: Caminho, feitos: u8) -> Vec<String> {
    let mut somas: Vec<(av::Efeito, f32)> = Vec::new();
    for k in 0..feitos.min(NOS_POR_CAMINHO) {
        let no = &av::NOS[c as usize][k as usize];
        match somas.iter_mut().find(|(e, _)| *e == no.efeito) {
            Some((_, v)) => *v += no.valor,
            None => somas.push((no.efeito, no.valor)),
        }
    }
    somas
        .into_iter()
        .map(|(e, v)| e.modelo().replace("{}", &e.numero(v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn com(nos: [u8; 4], ganhos: u32) -> ArvoreUi {
        let mut ui = ArvoreUi::default();
        ui.receber(EstadoArvore::de(av::Arvore::com_nos(nos), ganhos));
        ui
    }

    #[test]
    fn so_aprende_o_proximo_com_ponto_livre() {
        let ui = ArvoreUi::default();
        assert!(!ui.pode_aprender(0, 0), "no state, no promise");
        let ui = com([2, 0, 0, 0], 5);
        assert!(ui.pode_aprender(0, 2));
        assert!(!ui.pode_aprender(0, 1), "already learned");
        assert!(!ui.pode_aprender(0, 3), "out of order");
        assert!(ui.pode_aprender(3, 0));
        assert!(!ui.pode_aprender(9, 0));
        let ui = com([2, 0, 0, 0], 2);
        assert!(!ui.pode_aprender(0, 2), "no free point");
        assert!(!ui.tem_ponto_livre());
        assert!(com([2, 0, 0, 0], 3).tem_ponto_livre());
        assert!(!com([15, 15, 15, 15], 90).tem_ponto_livre(), "a full tree has nothing to spend on");
    }

    #[test]
    fn abre_no_primeiro_caminho_com_no_a_aprender() {
        let mut ui = com([15, 4, 0, 0], 30);
        ui.abrir();
        assert_eq!(ui.selecionado, (1, 4));
    }

    #[test]
    fn linhas_somam_por_efeito() {
        assert_eq!(linha_do_no(&av::NOS[0][0]), "Crit chance +1%");
        assert!(linhas_do_caminho(Caminho::Guardiao, 0).is_empty());
        let l = linhas_do_caminho(Caminho::Guardiao, 15);
        assert!(l.contains(&"Defense +20%".to_string()), "{l:?}");
        assert!(l.contains(&"Health regen +2/s".to_string()), "{l:?}");
    }
}

/// Local capture of the screen, with no connection.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-arvore-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut ui = com_previa();
    ui.abrir();
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha(12_500);
        unsafe { get_internal_gl().flush() };
        rt.texture.get_texture_data().export_png(&format!("{saida}/arvore.png"));
        next_frame().await;
    }
    ui.confirmar_redistribuir = true;
    crate::render3d::camera_padrao();
    clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
    ui.desenha(12_500);
    unsafe { get_internal_gl().flush() };
    rt.texture.get_texture_data().export_png(&format!("{saida}/arvore-reset.png"));
}

#[cfg(debug_assertions)]
fn com_previa() -> ArvoreUi {
    let mut ui = ArvoreUi::default();
    ui.receber(EstadoArvore::de(av::Arvore::com_nos([6, 0, 10, 3]), 23));
    ui
}
