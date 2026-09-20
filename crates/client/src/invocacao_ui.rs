//! Revelação de pergaminhos de invocação. O prêmio já chega decidido pelo
//! servidor; aqui só há apresentação, portanto fechar/pular nunca muda o ganho.

use macroquad::material::Material;
use macroquad::prelude::*;
use shared::loja::PremioInvocacao;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

/// Pergaminho fechado usado na loja e na bolsa. Tem rolos, fitas e selo em
/// camadas para continuar legível mesmo num slot pequeno.
pub fn icone_pergaminho(c: Vec2, lado: f32, selo: Color) {
    let w = lado * 0.58;
    let h = lado * 0.76;
    let sombra = Color::new(0.08, 0.03, 0.10, 0.52);
    let papel_escuro = Color::new(0.42, 0.23, 0.12, 1.0);
    let papel = Color::new(0.96, 0.79, 0.43, 1.0);
    let luz = Color::new(1.0, 0.92, 0.66, 1.0);
    draw_rectangle(c.x - w * 0.43 + lado * 0.05, c.y - h * 0.5 + lado * 0.06, w * 0.86, h, sombra);
    draw_rectangle(c.x - w * 0.43, c.y - h * 0.5, w * 0.86, h, papel);
    draw_triangle(
        vec2(c.x - w * 0.43, c.y - h * 0.5),
        vec2(c.x - w * 0.15, c.y - h * 0.5),
        vec2(c.x - w * 0.43, c.y - h * 0.29),
        papel_escuro,
    );
    for y in [-h * 0.50, h * 0.50] {
        draw_rectangle(c.x - w * 0.58, c.y + y - lado * 0.055, w * 1.16, lado * 0.11, papel_escuro);
        draw_circle(c.x - w * 0.58, c.y + y, lado * 0.07, luz);
        draw_circle(c.x + w * 0.58, c.y + y, lado * 0.07, luz);
    }
    for dy in [-0.18, -0.04, 0.10] {
        draw_line(c.x - w * 0.27, c.y + h * dy, c.x + w * 0.27, c.y + h * dy, lado * 0.018, papel_escuro);
    }
    draw_circle(c.x, c.y + h * 0.29, lado * 0.16, Color::new(0.20, 0.04, 0.08, 1.0));
    draw_circle(c.x, c.y + h * 0.29, lado * 0.125, selo);
    draw_poly(c.x, c.y + h * 0.29, 4, lado * 0.065, 45.0, luz);
}

/// Varia o selo por conteúdo: chave, cabeça de montaria ou livro aberto.
pub fn icone_pergaminho_de(item_id: u16, c: Vec2, lado: f32) {
    use shared::item_id as it;
    let cor = match item_id {
        it::PERGAMINHO_INVOCA_MONTARIA => estilo::OURO,
        it::PERGAMINHO_INVOCA_TOMO => estilo::VERDE,
        _ => Color::new(0.35, 0.78, 1.0, 1.0),
    };
    icone_pergaminho(c, lado, cor);
    let y = c.y + lado * 0.76 * 0.29;
    let tinta = Color::new(0.14, 0.07, 0.12, 0.95);
    match item_id {
        it::PERGAMINHO_INVOCA_CHAVE => {
            draw_circle_lines(c.x - lado * 0.025, y, lado * 0.045, lado * 0.018, tinta);
            draw_line(c.x + lado * 0.02, y, c.x + lado * 0.09, y, lado * 0.018, tinta);
            draw_line(c.x + lado * 0.07, y, c.x + lado * 0.07, y + lado * 0.035, lado * 0.014, tinta);
        }
        it::PERGAMINHO_INVOCA_MONTARIA => {
            draw_triangle(vec2(c.x, y - lado * 0.055), vec2(c.x - lado * 0.07, y + lado * 0.04), vec2(c.x, y + lado * 0.015), tinta);
            draw_triangle(vec2(c.x, y - lado * 0.055), vec2(c.x + lado * 0.07, y + lado * 0.04), vec2(c.x, y + lado * 0.015), tinta);
        }
        it::PERGAMINHO_INVOCA_TOMO => {
            draw_triangle(vec2(c.x, y + lado * 0.045), vec2(c.x - lado * 0.085, y - lado * 0.025), vec2(c.x, y - lado * 0.005), tinta);
            draw_triangle(vec2(c.x, y + lado * 0.045), vec2(c.x + lado * 0.085, y - lado * 0.025), vec2(c.x, y - lado * 0.005), tinta);
            draw_line(c.x, y - lado * 0.025, c.x, y + lado * 0.045, lado * 0.012, tinta);
        }
        _ => {}
    }
}

#[derive(Default)]
pub struct InvocacaoUi {
    atual: Option<(Vec<PremioInvocacao>, f64)>,
}

impl InvocacaoUi {
    pub fn abrir(&mut self, premio: PremioInvocacao, agora: f64) {
        self.abrir_varias(vec![premio], agora);
    }

    pub fn abrir_varias(&mut self, premios: Vec<PremioInvocacao>, agora: f64) {
        if !premios.is_empty() {
            self.atual = Some((premios, agora));
        }
    }

    pub fn aberta(&self) -> bool {
        self.atual.is_some()
    }

    pub fn fechar(&mut self) {
        self.atual = None;
    }

    pub fn desenha(
        &mut self,
        nomes: &std::collections::HashMap<u16, String>,
        skills: &[shared::skills::Skill],
        vox: &VoxCache,
        solido: &Material,
        agora: f64,
    ) {
        let Some((premios, inicio)) = self.atual.clone() else {
            return;
        };
        let t = (agora - inicio).max(0.0) as f32;
        let seguro = crate::hud_layout::tela_segura();
        crate::hud_layout::escurece((0.62 + (t * 2.0).min(0.25)).min(0.87));
        let c = seguro.center();
        let k = estilo::escala_do_painel(760.0, 430.0)
            .min((seguro.w - 16.0) / 760.0)
            .min((seguro.h - 16.0) / 430.0);
        let r = Rect::new(c.x - 380.0 * k, c.y - 215.0 * k, 760.0 * k, 430.0 * k);
        estilo::painel_destaque(r, estilo::OURO);

        let abre = (t / 1.25).clamp(0.0, 1.0);
        let revela = ((t - 1.25) / 0.75).clamp(0.0, 1.0);
        let pulso = ((t * 5.0).sin() * 0.5 + 0.5) * (1.0 - revela);
        for i in 0..18 {
            let a = i as f32 / 18.0 * std::f32::consts::TAU + t * (0.35 + (i % 3) as f32 * 0.08);
            let d = (75.0 + (i % 5) as f32 * 24.0 + pulso * 18.0) * k;
            draw_circle(
                c.x + a.cos() * d,
                c.y + a.sin() * d,
                (2.0 + (i % 3) as f32) * k,
                Color::new(1.0, 0.78, 0.32, 0.18 + 0.55 * revela),
            );
        }
        draw_circle(
            c.x,
            c.y,
            (70.0 + 90.0 * revela) * k,
            Color::new(0.40, 0.68, 1.0, 0.13 * revela),
        );

        if revela < 0.99 {
            let giro = abre * std::f32::consts::TAU;
            let raio = (48.0 + abre * 118.0) * k;
            for i in 0..12 {
                let a = giro + i as f32 * std::f32::consts::TAU / 12.0;
                let p1 = c + vec2(a.cos(), a.sin()) * raio * 0.62;
                let p2 = c + vec2(a.cos(), a.sin()) * raio;
                draw_line(p1.x, p1.y, p2.x, p2.y, (1.0 + 3.0 * abre) * k, estilo::alfa(estilo::OURO, 0.2 + abre * 0.7));
            }
            draw_circle(c.x, c.y, (38.0 + 72.0 * abre) * k, Color::new(1.0, 0.78, 0.28, 0.10 + abre * 0.18));
            let tremor = (t * 38.0).sin() * 2.5 * abre * k;
            icone_pergaminho(c + vec2(tremor, 0.0), (205.0 - 32.0 * abre) * k, estilo::OURO);
            // A fenda de luz cresce no centro até apagar o pergaminho e revelar.
            draw_rectangle(
                c.x - (2.0 + 30.0 * abre) * k,
                c.y - 92.0 * k,
                (4.0 + 60.0 * abre) * k,
                184.0 * k,
                Color::new(1.0, 0.94, 0.72, abre * 0.82),
            );
            estilo::texto_centro_forte(c.x, r.y + 38.0 * k, "ABRINDO PERGAMINHO", 22, estilo::OURO);
            estilo::texto_centro(
                c.x,
                r.y + r.h - 24.0 * k,
                "toque para revelar",
                13,
                estilo::SUAVE,
            );
        } else {
            if premios.len() > 1 {
                let revelados = (((t - 1.25).max(0.0) / 0.10) as usize).min(premios.len());
                let cols = 4usize;
                let cw = 150.0 * k;
                let ch = 86.0 * k;
                let x0 = c.x - cw * 2.0;
                let y0 = c.y - ch * 1.5;
                for (i, premio) in premios.iter().enumerate() {
                    let x = x0 + (i % cols) as f32 * cw;
                    let y = y0 + (i / cols) as f32 * ch;
                    let card = Rect::new(x + 4.0 * k, y + 4.0 * k, cw - 8.0 * k, ch - 8.0 * k);
                    let visivel = i < revelados;
                    estilo::cartao(card, false, visivel);
                    if !visivel {
                        icone_pergaminho(card.center(), 54.0 * k, Color::new(0.72, 0.62, 1.0, 1.0));
                        continue;
                    }
                    let (titulo, subtitulo, cor) = match premio {
                        PremioInvocacao::Chave { item_id, cor } => (
                            nomes.get(item_id).cloned().unwrap_or_else(|| "Chave".into()),
                            shared::forja::Grau::de_u8(*cor).map_or("Comum", |g| g.nome()).to_string(),
                            Color::from_rgba(80, 170, 255, 255),
                        ),
                        PremioInvocacao::Montaria { id, quantidade } => (
                            shared::loja::montaria(*id).map_or("Montaria", |m| m.nome).to_string(),
                            format!("agora x{quantidade}"),
                            estilo::OURO,
                        ),
                        PremioInvocacao::Tomo { skill_id, grau, quantidade } => (
                            skills.iter().find(|s| s.id == *skill_id).map_or("Tomo", |s| s.nome.as_str()).to_string(),
                            format!("{} · agora x{quantidade}", grau.nome()),
                            match grau {
                                shared::skills::GrauTomo::Verde => estilo::VERDE,
                                shared::skills::GrauTomo::Roxo => Color::new(0.72, 0.42, 1.0, 1.0),
                                shared::skills::GrauTomo::Lendario => estilo::OURO,
                            },
                        ),
                        PremioInvocacao::Pet { item_id } => (
                            shared::pets::de_item(*item_id)
                                .map_or("Pet".into(), |(e, _)| e.nome.to_string()),
                            shared::pets::de_item(*item_id).map_or(String::new(), |(_, g)| {
                                format!("{} · coletor", shared::pets::nome_do_grau(g))
                            }),
                            cor_do_grau(
                                shared::pets::de_item(*item_id).map_or(1, |(_, g)| g),
                            ),
                        ),
                    };
                    draw_circle(card.x + 22.0 * k, card.center().y, 12.0 * k, cor);
                    estilo::texto_ajustado(&titulo, card.x + 42.0 * k, card.y + 29.0 * k, card.w - 48.0 * k, 14, estilo::TEXTO);
                    estilo::texto(card.x + 42.0 * k, card.y + 55.0 * k, &subtitulo, 11, cor);
                }
                estilo::texto_centro_forte(c.x, r.y + 42.0 * k, "11 PRÊMIOS · BÔNUS 10+1", 24, estilo::OURO);
            } else {
            estilo::texto_centro_forte(
                c.x,
                r.y + 42.0 * k,
                "INVOCAÇÃO CONCLUÍDA",
                24,
                estilo::OURO,
            );
            match premios[0].clone() {
                PremioInvocacao::Chave { item_id, cor } => {
                    let q = Rect::new(c.x - 92.0 * k, c.y - 102.0 * k, 184.0 * k, 184.0 * k);
                    crate::icones::icone(item_id, q, Some(cor), None);
                    let nome = nomes
                        .get(&item_id)
                        .cloned()
                        .unwrap_or_else(|| "Chave de Craft".into());
                    estilo::texto_centro_forte(c.x, c.y + 105.0 * k, &nome, 23, estilo::TEXTO);
                    estilo::texto_centro(
                        c.x,
                        c.y + 132.0 * k,
                        &format!(
                            "Grau {}",
                            shared::forja::Grau::de_u8(cor).map_or("Comum", |g| g.nome())
                        ),
                        15,
                        estilo::SUAVE,
                    );
                }
                PremioInvocacao::Montaria { id, quantidade } => {
                    if let Some(m) = shared::loja::montaria(id) {
                        crate::render3d::vitrine_montaria(
                            vox,
                            m.skin_padrao,
                            Rect::new(c.x - 170.0 * k, c.y - 135.0 * k, 340.0 * k, 245.0 * k),
                            t * 0.45,
                            solido,
                        );
                        estilo::texto_centro_forte(c.x, c.y + 113.0 * k, m.nome, 25, estilo::TEXTO);
                        let estado = if quantidade > 1 {
                            format!("Cópia para aprimoramento · você possui x{quantidade}")
                        } else {
                            "Nova montaria liberada · você possui x1".into()
                        };
                        estilo::texto_centro(
                            c.x,
                            c.y + 143.0 * k,
                            &estado,
                            14,
                            if quantidade > 1 {
                                estilo::ACENTO
                            } else {
                                estilo::VERDE
                            },
                        );
                    }
                }
                PremioInvocacao::Pet { item_id } => {
                    let (especie, grau) = match shared::pets::de_item(item_id) {
                        Some(v) => v,
                        None => return,
                    };
                    let cor = cor_do_grau(grau);
                    brilho_livro(c, 105.0 * k, cor, t);
                    estilo::texto_centro_forte(
                        c.x,
                        c.y + 105.0 * k,
                        &format!("{} {}", especie.nome, shared::pets::nome_do_grau(grau)),
                        24,
                        cor,
                    );
                    estilo::texto_centro(
                        c.x,
                        c.y + 133.0 * k,
                        especie.descricao,
                        15,
                        estilo::TEXTO,
                    );
                    estilo::texto_centro(
                        c.x,
                        c.y + 158.0 * k,
                        &format!(
                            "Equipe no slot do pet: busca o saque a {:.0} tiles e dá {} pontos",
                            shared::pets::raio_de_busca(grau),
                            shared::pets::pontos(grau)
                        ),
                        13,
                        estilo::SUAVE,
                    );
                }
                PremioInvocacao::Tomo { skill_id, grau, quantidade } => {
                    let cor = match grau {
                        shared::skills::GrauTomo::Verde => estilo::VERDE,
                        shared::skills::GrauTomo::Roxo => Color::new(0.72, 0.42, 1.0, 1.0),
                        shared::skills::GrauTomo::Lendario => estilo::OURO,
                    };
                    let nome = skills
                        .iter()
                        .find(|s| s.id == skill_id)
                        .map_or("Habilidade", |s| s.nome.as_str());
                    brilho_livro(c, 105.0 * k, cor, t);
                    estilo::texto_centro_forte(c.x, c.y + 105.0 * k, &format!("Tomo {}", grau.nome()), 24, cor);
                    estilo::texto_centro(c.x, c.y + 133.0 * k, &format!("{nome} · você possui x{quantidade}"), 15, estilo::TEXTO);
                }
            }
            }
            let pronto = premios.len() == 1 || t >= 1.25 + premios.len() as f32 * 0.10;
            let bt = Rect::new(c.x - 115.0 * k, r.y + r.h - 52.0 * k, 230.0 * k, 39.0 * k);
            estilo::botao(
                bt,
                if pronto { "GUARDAR" } else { "REVELANDO…" },
                estilo::estado_de(bt, !pronto, false),
                pronto,
            );
            if pronto
                && is_mouse_button_pressed(MouseButton::Left)
                && bt.contains(Vec2::from(mouse_position()))
            {
                self.fechar();
            }
        }
        if t < 2.0 && is_mouse_button_pressed(MouseButton::Left) {
            self.atual = Some((premios, agora - 3.0));
        }
    }
}

fn brilho_livro(c: Vec2, lado: f32, cor: Color, t: f32) {
    let p = 1.0 + (t * 4.0).sin() * 0.05;
    draw_circle(c.x, c.y - 12.0, lado * 0.72 * p, Color::new(cor.r, cor.g, cor.b, 0.16));
    let w = lado * 0.92;
    let h = lado * 0.68;
    draw_rectangle(c.x - w, c.y - h * 0.5, w, h, Color::new(0.10, 0.08, 0.16, 1.0));
    draw_rectangle(c.x, c.y - h * 0.5, w, h, Color::new(0.13, 0.10, 0.20, 1.0));
    draw_triangle(vec2(c.x, c.y + h * 0.5), vec2(c.x - w, c.y + h * 0.5), vec2(c.x, c.y + h * 0.34), cor);
    draw_triangle(vec2(c.x, c.y + h * 0.5), vec2(c.x + w, c.y + h * 0.5), vec2(c.x, c.y + h * 0.34), cor);
    draw_line(c.x, c.y - h * 0.48, c.x, c.y + h * 0.42, 3.0, cor);
    draw_poly(c.x, c.y - 4.0, 6, lado * 0.20, t * 30.0, cor);
}

/// `MMO_PREVIA_INVOCACAO=1`: captura a abertura e os dois tipos de prêmio,
/// sem servidor.
#[cfg(debug_assertions)]
pub async fn previa(vox: &VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA")
        .unwrap_or_else(|_| "/tmp/tempest-invocacao-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    crate::render3d::define_alvo(Some(rt.clone()));
    let solido = crate::render3d::material_solido();
    let nomes =
        std::collections::HashMap::from([(shared::item_id::HORN + 2, "Chifre Azul".to_string())]);
    let lote: Vec<_> = (0..11)
        .map(|i| PremioInvocacao::Tomo {
            skill_id: i % 12 + 1,
            grau: shared::skills::GrauTomo::TODOS[(i as usize / 4).min(2)],
            quantidade: i as u16 + 1,
        })
        .collect();
    let cenas = [
        (
            "1-abrindo.png",
            vec![PremioInvocacao::Montaria {
                id: 2,
                quantidade: 1,
            }],
            0.55,
        ),
        (
            "2-chave.png",
            vec![PremioInvocacao::Chave {
                item_id: shared::item_id::HORN + 2,
                cor: 3,
            }],
            2.2,
        ),
        (
            "3-montaria.png",
            vec![PremioInvocacao::Montaria {
                id: 2,
                quantidade: 3,
            }],
            2.2,
        ),
        (
            "4-tomo.png",
            vec![PremioInvocacao::Tomo {
                skill_id: 12,
                grau: shared::skills::GrauTomo::Lendario,
                quantidade: 2,
            }],
            2.5,
        ),
        ("5-lote-10-mais-1.png", lote, 3.0),
    ];
    for (arquivo, premios, tempo) in cenas {
        let agora = get_time();
        let mut ui = InvocacaoUi::default();
        ui.abrir_varias(premios, agora - tempo);
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(&nomes, &shared::skills::playtest(), vox, &solido, agora);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{arquivo}"));
            next_frame().await;
        }
    }
}

/// A cor do grau do pet: a mesma tabela de cor dos itens.
fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}
