//! Revelação de pergaminhos de invocação. O prêmio já chega decidido pelo
//! servidor; aqui só há apresentação, portanto fechar/pular nunca muda o ganho.

use macroquad::material::Material;
use macroquad::prelude::*;
use shared::loja::PremioInvocacao;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

/// Pergaminho fechado usado na loja; desenho vetorial leve, sem asset novo.
pub fn icone_pergaminho(c: Vec2, lado: f32, selo: Color) {
    let w = lado * 0.54;
    let h = lado * 0.78;
    let papel = Color::new(0.84, 0.67, 0.34, 1.0);
    let borda = Color::new(0.98, 0.80, 0.42, 1.0);
    draw_rectangle(c.x - w * 0.5, c.y - h * 0.5, w, h, papel);
    draw_rectangle_lines(
        c.x - w * 0.5,
        c.y - h * 0.5,
        w,
        h,
        lado * 0.025,
        borda,
    );
    draw_circle(c.x - w * 0.5, c.y - h * 0.38, lado * 0.09, borda);
    draw_circle(c.x + w * 0.5, c.y + h * 0.38, lado * 0.09, borda);
    draw_line(
        c.x - w * 0.28,
        c.y - h * 0.15,
        c.x + w * 0.28,
        c.y - h * 0.15,
        lado * 0.035,
        Color::new(0.37, 0.20, 0.12, 0.75),
    );
    draw_circle(c.x, c.y + h * 0.16, lado * 0.13, selo);
    draw_circle_lines(
        c.x,
        c.y + h * 0.16,
        lado * 0.13,
        lado * 0.018,
        borda,
    );
}

#[derive(Default)]
pub struct InvocacaoUi {
    atual: Option<(PremioInvocacao, f64)>,
}

impl InvocacaoUi {
    pub fn abrir(&mut self, premio: PremioInvocacao, agora: f64) {
        self.atual = Some((premio, agora));
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
        vox: &VoxCache,
        solido: &Material,
        agora: f64,
    ) {
        let Some((premio, inicio)) = self.atual.clone() else {
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
            let largura = (64.0 + 160.0 * abre) * k;
            let alto = 220.0 * k;
            draw_rectangle(
                c.x - largura * 0.5,
                c.y - alto * 0.5,
                largura,
                alto,
                Color::new(0.83, 0.66, 0.34, 1.0),
            );
            draw_rectangle_lines(
                c.x - largura * 0.5,
                c.y - alto * 0.5,
                largura,
                alto,
                3.0 * k,
                estilo::OURO,
            );
            draw_circle(c.x - largura * 0.5, c.y, 18.0 * k, estilo::OURO);
            draw_circle(c.x + largura * 0.5, c.y, 18.0 * k, estilo::OURO);
            let selo = (1.0 - abre) * 32.0 * k;
            draw_circle(
                c.x,
                c.y,
                selo.max(5.0 * k),
                Color::new(0.48, 0.08, 0.12, 1.0),
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
            estilo::texto_centro_forte(
                c.x,
                r.y + 42.0 * k,
                "INVOCAÇÃO CONCLUÍDA",
                24,
                estilo::OURO,
            );
            match premio {
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
            }
            let bt = Rect::new(c.x - 115.0 * k, r.y + r.h - 52.0 * k, 230.0 * k, 39.0 * k);
            estilo::botao(bt, "GUARDAR", estilo::estado_de(bt, false, false), true);
            if is_mouse_button_pressed(MouseButton::Left)
                && bt.contains(Vec2::from(mouse_position()))
            {
                self.fechar();
            }
        }
        if t < 2.0 && is_mouse_button_pressed(MouseButton::Left) {
            self.atual = Some((premio, agora - 2.1));
        }
    }
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
    let cenas = [
        (
            "1-abrindo.png",
            PremioInvocacao::Montaria {
                id: 2,
                quantidade: 1,
            },
            0.55,
        ),
        (
            "2-chave.png",
            PremioInvocacao::Chave {
                item_id: shared::item_id::HORN + 2,
                cor: 3,
            },
            2.2,
        ),
        (
            "3-montaria.png",
            PremioInvocacao::Montaria {
                id: 2,
                quantidade: 3,
            },
            2.2,
        ),
    ];
    for (arquivo, premio, tempo) in cenas {
        let agora = get_time();
        let mut ui = InvocacaoUi::default();
        ui.abrir(premio, agora - tempo);
        for _ in 0..2 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(&nomes, vox, &solido, agora);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{arquivo}"));
            next_frame().await;
        }
    }
}
