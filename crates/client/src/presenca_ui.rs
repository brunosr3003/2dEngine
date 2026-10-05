//! Attendance calendar (docs/CALENDARIO.md): the grid of the month's 28
//! prizes, today highlighted, the claimed ones marked and the Claim button.
//! Opens from the icon at the top of the HUD or from the Menu (Adventure ->
//! Attendance) and, once per session, on its own at login when there is something to claim.
//!
//! Everything that counts is the server's: the window only shows state and asks.
use std::collections::HashMap;

use macroquad::prelude::*;
use shared::presenca::{self as pr, AvisoPresenca, EstadoPresenca, PedidoPresenca, Premio};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

const COLUNAS: usize = 7;
/// How much wider the last column (the bonus days) is than the others.
const LARGURA_DO_BONUS: f32 = 1.3;

#[derive(Default)]
pub struct PresencaUi {
    pub aberto: bool,
    estado: Option<EstadoPresenca>,
    aba: usize,
    /// Already opened on its own in this app session.
    abriu_no_login: bool,
    /// Ultimo resgate, mostrado no rodape.
    ultimo: Option<String>,
}

/// "Experience Potion x2" / "1,500 gold".
pub fn texto_do_premio(p: &Premio, nomes: &HashMap<u16, String>) -> String {
    if p.item_id == pr::ENERGIA {
        return format!("{} Energy", crate::economia::milhar(p.qtd as u64));
    }
    if p.item_id == pr::OURO {
        return format!("{} gold", crate::economia::milhar(p.qtd as u64));
    }
    let nome = nomes
        .get(&p.item_id)
        .cloned()
        .unwrap_or_else(|| format!("Item {}", p.item_id));
    format!("{nome} ×{}", p.qtd)
}

/// The prize's readable name, whole: the card has room for it under the
/// icon, so nothing is cut to "…" any more (owner, 05/10/2026: make it clear
/// what you receive).
fn nome_do_premio(p: &Premio, nomes: &HashMap<u16, String>) -> String {
    use shared::item_id::*;
    match p.item_id {
        pr::ENERGIA => "Energy".into(),
        pr::OURO => "Gold".into(),
        PERGAMINHO_INVOCA_PET => "Pet Summon Scroll".into(),
        PERGAMINHO_INVOCA_MONTARIA => "Mount Summon Scroll".into(),
        PASSE_MAGICO => "Magic Island Pass".into(),
        COPPER => "Copper".into(),
        DARKSTEEL => "Darksteel".into(),
        GLITTERING_POWDER => "Glittering Dust".into(),
        id => nomes.get(&id).cloned().unwrap_or_else(|| format!("Item {id}")),
    }
}

/// The name ON THE CARD: whole on a card with one prize ("Pet Scroll"),
/// one word on a card that splits its width between two or three. The
/// reward box and the tooltip carry the full name.
fn nome_no_cartao(p: &Premio, nomes: &HashMap<u16, String>, divide: bool) -> String {
    use shared::item_id::*;
    match (p.item_id, divide) {
        (PERGAMINHO_INVOCA_PET, false) => "Pet Scroll".into(),
        (PERGAMINHO_INVOCA_MONTARIA, false) => "Mount Scroll".into(),
        (PASSE_MAGICO, false) => "Magic Pass".into(),
        (PERGAMINHO_INVOCA_PET, true) => "Pet".into(),
        (PERGAMINHO_INVOCA_MONTARIA, true) => "Mount".into(),
        (PASSE_MAGICO, true) => "Pass".into(),
        (GLITTERING_POWDER, true) => "Dust".into(),
        _ => nome_do_premio(p, nomes),
    }
}

/// "10k" / "5k" / "150": the amount on a card shared by two prizes.
fn quantia_curta(p: &Premio) -> String {
    if p.qtd >= 1_000 && p.qtd % 1_000 == 0 {
        format!("{}k", p.qtd / 1_000)
    } else {
        quantia(p)
    }
}

/// "5,000" — the amount alone, big under the icon.
fn quantia(p: &Premio) -> String {
    crate::economia::milhar(p.qtd as u64)
}

/// The prize's icon in `r`. The summon scrolls have no atlas drawing, so
/// they get a parchment with a coloured seal (green pet, blue mount) instead
/// of an empty square — the milestone days were blank.
fn icone_do_premio(p: &Premio, r: Rect, vox: &crate::vox::VoxCache, solido: &macroquad::material::Material) {
    use shared::item_id::*;
    match p.item_id {
        pr::ENERGIA => estilo::icone_energia(r.center(), r.w.min(r.h)),
        PERGAMINHO_INVOCA_PET | PERGAMINHO_INVOCA_MONTARIA => {
            let selo = if p.item_id == PERGAMINHO_INVOCA_PET {
                Color::new(0.36, 0.78, 0.42, 1.0)
            } else {
                Color::new(0.38, 0.62, 0.98, 1.0)
            };
            let l = r.w.min(r.h);
            let papel = Rect::new(r.center().x - l * 0.30, r.center().y - l * 0.38, l * 0.60, l * 0.76);
            estilo::ret_arredondado(papel, l * 0.05, Color::new(0.93, 0.85, 0.66, 1.0));
            for yy in [papel.y - l * 0.04, papel.y + papel.h - l * 0.06] {
                estilo::ret_arredondado(
                    Rect::new(papel.x - l * 0.06, yy, papel.w + l * 0.12, l * 0.10),
                    l * 0.05,
                    Color::new(0.72, 0.55, 0.32, 1.0),
                );
            }
            for k in 0..3 {
                let yl = papel.y + papel.h * (0.28 + 0.14 * k as f32);
                draw_line(papel.x + l * 0.08, yl, papel.x + papel.w - l * 0.08, yl, 1.5, Color::new(0.55, 0.43, 0.27, 0.8));
            }
            draw_circle(papel.center().x, papel.y + papel.h * 0.78, l * 0.12, selo);
            draw_circle_lines(papel.center().x, papel.y + papel.h * 0.78, l * 0.12, 1.5, Color::new(0.0, 0.0, 0.0, 0.35));
        }
        // The Magic Island pass: a violet ticket with a star.
        PASSE_MAGICO => {
            let l = r.w.min(r.h);
            let t = Rect::new(r.center().x - l * 0.42, r.center().y - l * 0.28, l * 0.84, l * 0.56);
            estilo::ret_arredondado(t, l * 0.08, Color::new(0.55, 0.32, 0.86, 1.0));
            estilo::borda_arredondada(t, l * 0.08, 1.5, Color::new(0.90, 0.80, 1.0, 0.9));
            draw_circle(t.x, t.center().y, l * 0.07, estilo::FUNDO_BAIXO);
            draw_circle(t.x + t.w, t.center().y, l * 0.07, estilo::FUNDO_BAIXO);
            let c = t.center();
            let e = l * 0.16;
            draw_poly(c.x, c.y, 5, e, -90.0, Color::new(1.0, 0.86, 0.40, 1.0));
        }
        // The atlas draws with a margin of its own: a slightly bigger box
        // puts its icons at the size of the drawn ones beside them.
        id => crate::icones::icone_com_3d(
            if id == pr::OURO { GOLD } else { id },
            Rect::new(r.x - r.w * 0.2, r.y - r.h * 0.2, r.w * 1.4, r.h * 1.4),
            None,
            None,
            Some((vox, solido)),
        ),
    }
}

/// Centred text that SHRINKS to fit `largura` (down to 9) before it is cut:
/// a long name reads small rather than as "Mou…".
fn centro_que_cabe(s: &str, cx: f32, y: f32, largura: f32, tam: u16, cor: Color, forte: bool) {
    centro_que_cabe_ate(s, cx, y, largura, tam, 9, cor, forte)
}

/// `centro_que_cabe` with the smallest size allowed (a card split in three
/// has columns a third as wide).
#[allow(clippy::too_many_arguments)]
fn centro_que_cabe_ate(s: &str, cx: f32, y: f32, largura: f32, tam: u16, minimo: u16, cor: Color, forte: bool) {
    let t = shared::idioma::tr(s).into_owned();
    let mut tamanho = tam;
    let medida = |t: &str, z: u16| if forte { estilo::medir_forte(t, z) } else { estilo::medir(t, z) };
    while tamanho > minimo && medida(&t, tamanho) > largura {
        tamanho -= 1;
    }
    if medida(&t, tamanho) > largura {
        estilo::texto_ajustado(&t, cx - largura * 0.5, y, largura, tamanho, cor);
    } else if forte {
        estilo::texto_centro_forte(cx, y, &t, tamanho, cor);
    } else {
        estilo::texto_centro(cx, y, &t, tamanho, cor);
    }
}

/// A small tag on a card's top-right corner: TODAY, BONUS.
fn etiqueta(r: Rect, texto: &str, fundo: Color, cor: Color) {
    let f = estilo::fator_texto();
    let w = estilo::medir_forte(&shared::idioma::tr(texto), 9) + 8.0 * f;
    let t = Rect::new(r.x + r.w - w - 4.0 * f, r.y + 5.0 * f, w, 14.0 * f);
    estilo::ret_arredondado(t, 3.0 * f, fundo);
    estilo::texto_centro_forte(t.center().x, t.y + 10.5 * f, texto, 9, cor);
}

/// "5h 12m" / "12m".
fn falta(s: i64) -> String {
    let s = s.max(0);
    let (h, m) = (s / 3600, s / 60 % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

impl PresencaUi {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        vec![ClientMessage::Presenca {
            pedido: PedidoPresenca::Estado,
        }]
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    /// There is a prize to claim today (a red badge on the icon).
    pub fn tem_resgate(&self) -> bool {
        self.estado.as_ref().is_some_and(|e| e.tem_resgate())
    }

    /// Chegou aviso do servidor. Devolve texto pro chat.
    pub fn receber(
        &mut self,
        aviso: AvisoPresenca,
        nomes: &HashMap<u16, String>,
    ) -> Option<String> {
        match aviso {
            AvisoPresenca::Estado(e) => {
                if !self.abriu_no_login {
                    self.abriu_no_login = true;
                    if e.tem_resgate() {
                        self.aberto = true;
                    }
                }
                if self.aba >= e.calendarios.len() {
                    self.aba = 0;
                }
                self.estado = Some(e);
                None
            }
            AvisoPresenca::Resgatou {
                dia,
                premios,
                no_correio,
                ..
            } => {
                let lista: Vec<String> =
                    premios.iter().map(|p| texto_do_premio(p, nomes)).collect();
                // Day 0: the prize of a claim left pending (a crash before the save).
                let mut t = if dia == 0 {
                    format!("Presença · prêmio pendente entregue: {}", lista.join(", "))
                } else {
                    format!("Attendance · day {dia}: {}", lista.join(", "))
                };
                if no_correio > 0 {
                    t.push_str(" (bag full: part of it in your Mail)");
                }
                self.ultimo = Some(t.clone());
                Some(t)
            }
            AvisoPresenca::Recusado { texto } => {
                self.ultimo = Some(texto.clone());
                Some(texto)
            }
        }
    }

    /// Desenha e trata o toque. Devolve os pedidos pro servidor.
    pub fn desenha(
        &mut self,
        nomes: &HashMap<u16, String>,
        agora_unix: i64,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) -> Vec<ClientMessage> {
        estilo::no_painel(estilo::escala_do_painel(780.0, 640.0), || {
            self.desenha_na_escala(nomes, agora_unix, vox, solido)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nomes: &HashMap<u16, String>,
        agora_unix: i64,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        // MAIS LARGO E MENOS ALTO QUE ANTES.
        //
        // Era 780x640 com o cartao limitado a 1,15x a propria largura, e a
        // conta saia ao contrario do que o olho precisa: sobrava altura (um
        // cartao de um premio so' usava o terco de cima e deixava o resto
        // vazio) e faltava largura, onde o rotulo cabia em 122 px e virava
        // "Darksteel …". Nenhum premio dava pra ler, que e' a unica coisa que
        // o calendario tem pra dizer.
        let w = (980.0 * f).min(seguro.w - 16.0);
        let h = (680.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(
            x0,
            p.y + 36.0 * f,
            "Attendance calendar",
            20,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );
        if clicou && fechar.contains(m) {
            self.fechar();
            return saida;
        }
        let Some(estado) = self.estado.clone() else {
            estilo::texto(x0, p.y + 80.0 * f, "Loading…", 15, estilo::SUAVE);
            return saida;
        };
        if estado.calendarios.is_empty() {
            estilo::texto(
                x0,
                p.y + 80.0 * f,
                "No active calendar.",
                15,
                estilo::SUAVE,
            );
            return saida;
        }
        // Tabs: the monthly one and each active event.
        let mut y = p.y + 52.0 * f;
        if estado.calendarios.len() > 1 {
            let mut x = x0;
            for (i, c) in estado.calendarios.iter().enumerate() {
                let tw = estilo::medir(&c.nome, 14) + 28.0 * f;
                let r = Rect::new(x, y, tw, 34.0 * f);
                estilo::cartao(r, r.contains(m), i == self.aba);
                estilo::texto_centro(
                    r.center().x,
                    r.center().y + 5.0 * f,
                    &c.nome,
                    14,
                    if i == self.aba {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    },
                );
                if c.pode_hoje {
                    crate::hud::selo(r);
                }
                if clicou && r.contains(m) {
                    self.aba = i;
                }
                x += tw + 8.0 * f;
            }
            y += 42.0 * f;
        }
        let cal = &estado.calendarios[self.aba.min(estado.calendarios.len() - 1)];
        let sub = if cal.fim_unix > 0 {
            format!(
                "{} · ends in {}",
                cal.nome,
                falta(cal.fim_unix - agora_unix)
            )
        } else {
            format!("{} · the month turns over on the 1st at 04:00", cal.nome)
        };
        estilo::texto(x0, y + 18.0 * f, &sub, 13, estilo::SUAVE);
        y += 30.0 * f;

        // The grid: 7 per row. Every card says WHAT, big: the icon in the
        // middle, the amount in bold under it and the whole name under that.
        let linhas = cal.grade.len().div_ceil(COLUNAS).max(1);
        let vao = 8.0 * f;
        let rodape = 128.0 * f;
        // The last column holds the bonus days (7, 14, 21, 28): it is wider,
        // so their longer prizes read whole and they stand out.
        let cw = (p.w - 40.0 * f - vao * (COLUNAS as f32 - 1.0)) / (COLUNAS as f32 - 1.0 + LARGURA_DO_BONUS);
        // Taller than wide: the icon, the amount and the name stack, and the
        // rows fill the panel down to the reward box.
        let ch = ((p.y + p.h - rodape - y - vao * (linhas as f32 - 1.0)) / linhas as f32).min(cw * 1.45);
        let proximo = cal.resgatados as usize;
        let verde = Color::new(0.45, 0.85, 0.52, 1.0);
        let mut dica: Option<(Rect, String)> = None;
        for (i, dia) in cal.grade.iter().enumerate() {
            let n = i as u8 + 1;
            let coluna_i = i % COLUNAS;
            let r = Rect::new(
                x0 + coluna_i as f32 * (cw + vao),
                y + (i / COLUNAS) as f32 * (ch + vao),
                if coluna_i == COLUNAS - 1 { cw * LARGURA_DO_BONUS } else { cw },
                ch,
            );
            let sobre = r.contains(m);
            let resgatado = (i as u8) < cal.resgatados;
            let hoje = i == proximo && cal.pode_hoje;
            let marco = pr::e_marco(n);
            estilo::cartao(r, sobre, hoje);
            if marco {
                estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, Color::new(0.91, 0.75, 0.46, 0.10));
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 2.0, estilo::OURO);
            }
            if hoje {
                estilo::borda_arredondada(r, estilo::RAIO_PEQUENO, 3.0, estilo::ACENTO);
            }
            estilo::texto_forte(
                r.x + 7.0 * f,
                r.y + 16.0 * f,
                &format!("{n}"),
                13,
                if marco || hoje { estilo::OURO } else { estilo::SUAVE },
            );
            if hoje {
                etiqueta(r, "TODAY", estilo::ACENTO, Color::new(0.12, 0.08, 0.04, 1.0));
            } else if marco && !resgatado {
                etiqueta(r, "BONUS", Color::new(0.55, 0.40, 0.18, 1.0), estilo::TEXTO);
            }
            let premios: Vec<&Premio> = dia.iter().filter(|p| p.qtd > 0).collect();
            let k = premios.len().max(1);
            let coluna = r.w / k as f32;
            // Bottom up, so the text never leaves the card: the name on the
            // bottom edge, the amount above it, the icon in all that is left.
            let y_nome = r.y + r.h - 8.0 * f;
            let y_qtd = y_nome - 15.0 * f;
            let topo = r.y + 24.0 * f;
            let base_icone = y_qtd - (if k == 1 { 18.0 } else { 15.0 }) * f;
            let lado = (base_icone - topo).min(coluna * 0.78).max(12.0 * f);
            // Three prizes (day 28) read as ROWS: icon, amount, name on a line
            // each — a third of the card's width cut "Mount" to "Mou…".
            if k >= 3 {
                let linha = (r.y + r.h - 6.0 * f - topo) / k as f32;
                let li = (linha * 0.72).min(21.0 * f);
                for (j, pp) in premios.iter().enumerate() {
                    let yy = topo + j as f32 * linha;
                    icone_do_premio(pp, Rect::new(r.x + 6.0 * f, yy + (linha - li) * 0.5, li, li), vox, solido);
                    let xt = r.x + 8.0 * f + li;
                    let base = yy + linha * 0.5 + 5.0 * f;
                    let q = quantia(pp);
                    estilo::texto_forte(xt, base, &q, 12, if marco { estilo::OURO } else { estilo::TEXTO });
                    let xn = xt + estilo::medir_forte(&q, 12) + 3.0 * f;
                    let nome = shared::idioma::tr(&nome_no_cartao(pp, nomes, true)).into_owned();
                    let livre = r.x + r.w - 3.0 * f - xn;
                    let mut z = 11;
                    while z > 8 && estilo::medir(&nome, z) > livre {
                        z -= 1;
                    }
                    estilo::texto_ajustado(&nome, xn, base, livre, z, estilo::SUAVE);
                }
            }
            for (j, pp) in premios.iter().enumerate().filter(|_| k < 3) {
                let cx = r.x + coluna * (j as f32 + 0.5);
                let ic = Rect::new(cx - lado * 0.5, base_icone - lado, lado, lado);
                icone_do_premio(pp, ic, vox, solido);
                // Two prizes share the width: the amount goes compact ("10k")
                // so it is never the part that gets cut.
                let q = if k == 1 { quantia(pp) } else { quantia_curta(pp) };
                centro_que_cabe_ate(&q, cx, y_qtd, coluna - 4.0 * f, if k == 1 { 17 } else { 14 }, 9, if marco { estilo::OURO } else { estilo::TEXTO }, true);
                centro_que_cabe_ate(&nome_no_cartao(pp, nomes, k > 1), cx, y_nome, coluna - 2.0 * f, 11, if k > 1 { 7 } else { 8 }, estilo::SUAVE, false);
            }
            if resgatado {
                estilo::ret_arredondado(r, estilo::RAIO_PEQUENO, Color::new(0.0, 0.0, 0.0, 0.55));
                let c = vec2(r.x + r.w - 15.0 * f, r.y + 14.0 * f);
                draw_circle(c.x, c.y, 10.0 * f, Color::new(0.10, 0.30, 0.14, 1.0));
                estilo::traco(vec2(c.x - 5.0 * f, c.y), vec2(c.x - 1.5 * f, c.y + 4.0 * f), 2.5, verde);
                estilo::traco(vec2(c.x - 1.5 * f, c.y + 4.0 * f), vec2(c.x + 5.0 * f, c.y - 4.0 * f), 2.5, verde);
            }
            if sobre {
                let t: Vec<String> = premios.iter().map(|p| texto_do_premio(p, nomes)).collect();
                dica = Some((
                    r,
                    format!(
                        "Day {n}{}: {}",
                        if marco { " (bonus)" } else { "" },
                        t.join(" + ")
                    ),
                ));
            }
        }

        // Footer: WHAT you get, in the open — today's prize (or tomorrow's,
        // once claimed) with its icons and whole names, next to the button.
        let yb = p.y + p.h - rodape + 10.0 * f;
        let caixa = Rect::new(x0, yb, p.w - 40.0 * f - 230.0 * f, rodape - 26.0 * f);
        estilo::ret_arredondado(caixa, estilo::RAIO_PEQUENO, estilo::FUNDO_BAIXO);
        estilo::borda_arredondada(caixa, estilo::RAIO_PEQUENO, 1.0, estilo::BORDA);
        let (titulo_caixa, dia_mostrado) = if cal.pode_hoje {
            ("Today's reward", Some(proximo))
        } else if proximo < cal.grade.len() {
            ("Next reward", Some(proximo))
        } else {
            ("All rewards claimed this month", None)
        };
        estilo::texto_forte(caixa.x + 12.0 * f, caixa.y + 22.0 * f, titulo_caixa, 14, estilo::OURO);
        let resumo = if cal.pode_hoje {
            format!("Day {} of {} · claimed {}/{}", proximo + 1, cal.grade.len(), cal.resgatados, cal.grade.len())
        } else {
            format!(
                "Claimed {}/{} · next claim in {}",
                cal.resgatados,
                cal.grade.len(),
                falta(estado.proximo_reset_unix - agora_unix)
            )
        };
        estilo::texto(caixa.x + 12.0 * f, caixa.y + caixa.h - 12.0 * f, &resumo, 12, estilo::SUAVE);
        if let Some(d) = dia_mostrado.and_then(|d| cal.grade.get(d)) {
            let mut x = caixa.x + 12.0 * f;
            let lado = 40.0 * f;
            let yi = caixa.y + 32.0 * f;
            for pp in d.iter().filter(|p| p.qtd > 0) {
                icone_do_premio(pp, Rect::new(x, yi, lado, lado), vox, solido);
                // Translated before the amount joins it: the dictionary knows
                // "Darksteel", not "Darksteel ×5,000".
                let texto = format!("{} ×{}", shared::idioma::tr(&nome_do_premio(pp, nomes)), quantia(pp));
                let tw = estilo::medir_forte(&shared::idioma::tr(&texto), 15);
                estilo::texto_forte(x + lado + 8.0 * f, yi + lado * 0.62, &texto, 15, estilo::TEXTO);
                x += lado + tw + 28.0 * f;
            }
        }
        let bot = Rect::new(
            p.x + p.w - 20.0 * f - 214.0 * f,
            yb + 8.0 * f,
            214.0 * f,
            62.0 * f,
        );
        let ativo = cal.pode_hoje;
        estilo::cartao(bot, ativo && bot.contains(m), ativo);
        if ativo {
            estilo::borda_arredondada(bot, estilo::RAIO_PEQUENO, 2.0, estilo::ACENTO);
        }
        estilo::texto_centro_forte(
            bot.center().x,
            bot.center().y + 8.0 * f,
            if ativo { "Claim" } else { "Claimed today" },
            if ativo { 20 } else { 15 },
            if ativo { estilo::OURO } else { estilo::SUAVE },
        );
        if let Some(t) = &self.ultimo {
            estilo::texto_ajustado(t, bot.x, bot.y + bot.h + 18.0 * f, bot.w, 11, estilo::SUAVE);
        }
        if ativo && clicou && bot.contains(m) {
            saida.push(ClientMessage::Presenca {
                pedido: PedidoPresenca::Resgatar { calendario: cal.id },
            });
        }
        if let Some((r, t)) = dica {
            estilo::tooltip(r, &t, true);
        }
        saida
    }
}

/// Previa do calendario de presenca (`MMO_PREVIA_PRESENCA=1`; PNGs em
/// `MMO_PREVIA_SAIDA`).
///
/// Dois estados, porque sao os dois que o jogador ve': com premio pra resgatar
/// hoje e sem. Uma captura so' nao mostra o botao nos dois modos.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-presenca-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let vox = crate::vox::VoxCache::default();
    let solido = crate::render3d::material_solido();
    let nomes: HashMap<u16, String> = HashMap::new();
    for (nome, pode) in [("presenca-com-resgate", true), ("presenca-sem-resgate", false)] {
        let mut ui = PresencaUi::default();
        let mut e = shared::presenca::DadosPresenca::default()
            .estado(1_789_000_000, pr::EVENTOS);
        for c in &mut e.calendarios {
            c.pode_hoje = pode;
            // The claimed look too: five days already taken.
            if !pode {
                c.resgatados = 5;
            }
        }
        ui.estado = Some(e);
        ui.aberto = true;
        ui.ultimo = Some("Experience Potion x2".into());
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(&nomes, 1_789_000_000, &vox, &solido);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado(pode: bool) -> EstadoPresenca {
        shared::presenca::DadosPresenca::default()
            .estado(1_789_000_000, pr::EVENTOS)
            .tap(pode)
    }

    trait Tap {
        fn tap(self, pode: bool) -> Self;
    }
    impl Tap for EstadoPresenca {
        fn tap(mut self, pode: bool) -> Self {
            for c in &mut self.calendarios {
                c.pode_hoje = pode;
            }
            self
        }
    }

    #[test]
    fn abre_sozinha_so_no_primeiro_estado_com_resgate() {
        let nomes = HashMap::new();
        let mut ui = PresencaUi::default();
        ui.receber(AvisoPresenca::Estado(estado(true)), &nomes);
        assert!(ui.aberto && ui.tem_resgate());
        ui.fechar();
        ui.receber(AvisoPresenca::Estado(estado(true)), &nomes);
        assert!(!ui.aberto, "so' uma vez por sessao");

        let mut sem = PresencaUi::default();
        sem.receber(AvisoPresenca::Estado(estado(false)), &nomes);
        assert!(!sem.aberto && !sem.tem_resgate());
    }

    #[test]
    fn texto_do_resgate_diz_premios_e_correio() {
        let mut nomes = HashMap::new();
        nomes.insert(
            shared::item_id::XP_POTION,
            "Experience Potion".to_string(),
        );
        let mut ui = PresencaUi::default();
        let t = ui
            .receber(
                AvisoPresenca::Resgatou {
                    calendario: 0,
                    dia: 7,
                    premios: vec![
                        Premio {
                            item_id: shared::item_id::XP_POTION,
                            qtd: 1,
                        },
                        Premio {
                            item_id: pr::OURO,
                            qtd: 1500,
                        },
                    ],
                    no_correio: 1,
                },
                &nomes,
            )
            .unwrap();
        assert!(
            t.contains("day 7")
                && t.contains("Experience Potion ×1")
                && t.contains("1,500 gold")
                && t.contains("Mail")
        );
    }

    #[test]
    fn contador() {
        assert_eq!(falta(5 * 3600 + 12 * 60 + 5), "5h 12m");
        assert_eq!(falta(600), "10m");
    }
}
