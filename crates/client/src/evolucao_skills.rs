//! Evolução das doze habilidades. A tela só apresenta custos e envia pedidos;
//! Energia, cobre, tomos e tier são sempre conferidos pelo servidor.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::skills::{self, Conjunto, ProgressoDeSkills, Skill};

use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct EvolucaoSkills {
    pub aberto: bool,
    pub progresso: ProgressoDeSkills,
    conjunto: usize,
    selecionada: Option<u32>,
    aviso: Option<String>,
}

impl EvolucaoSkills {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn resultado(&mut self, texto: String) {
        self.aviso = Some(texto);
    }

    pub fn desenha(&mut self, catalogo: &[Skill], nivel: u32, cobre: u32) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        let seguro = crate::hud_layout::tela_segura();
        let escala = estilo::escala_do_painel(820.0, 610.0)
            .min((seguro.w - 16.0) / 820.0)
            .min((seguro.h - 16.0) / 610.0);
        estilo::no_painel(escala, || self.desenha_na_escala(catalogo, nivel, cobre))
    }

    fn desenha_na_escala(
        &mut self,
        catalogo: &[Skill],
        nivel: u32,
        cobre: u32,
    ) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (820.0 * f).min(seguro.w - 16.0);
        let h = (610.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.5);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        estilo::texto_forte(
            p.x + 20.0 * f,
            p.y + 36.0 * f,
            "Evolução das habilidades",
            21,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(m) {
            self.fechar();
            return None;
        }
        estilo::texto(
            p.x + 20.0 * f,
            p.y + 65.0 * f,
            &format!("Energia  {}     Cobre  {}", self.progresso.energia, cobre),
            16,
            estilo::TEXTO,
        );

        let largura_aba = (p.w - 40.0 * f) / 4.0;
        for (i, conjunto) in Conjunto::TODOS.iter().enumerate() {
            let r = Rect::new(
                p.x + 20.0 * f + i as f32 * largura_aba,
                p.y + 78.0 * f,
                largura_aba - 5.0 * f,
                38.0 * f,
            );
            estilo::botao(
                r,
                conjunto.nome(),
                estilo::estado_de(r, false, self.conjunto == i),
                self.conjunto == i,
            );
            if clique && r.contains(m) {
                self.conjunto = i;
                self.selecionada = None;
            }
        }
        let conjunto = Conjunto::TODOS[self.conjunto];
        let mut habilidades: Vec<&Skill> =
            catalogo.iter().filter(|s| s.conjunto == conjunto).collect();
        habilidades.sort_by_key(|s| s.ordem);
        if self.selecionada.is_none() {
            self.selecionada = habilidades.first().map(|s| s.id);
        }
        for (i, skill) in habilidades.iter().enumerate() {
            let y = p.y + (132.0 + i as f32 * 91.0) * f;
            let r = Rect::new(p.x + 20.0 * f, y, 284.0 * f, 78.0 * f);
            estilo::cartao(r, r.contains(m), self.selecionada == Some(skill.id));
            estilo::texto_forte(
                r.x + 12.0 * f,
                r.y + 29.0 * f,
                &skill.nome,
                16,
                estilo::TEXTO,
            );
            let estado = if nivel < skill.nivel_necessario() {
                format!("Libera no nível {}", skill.nivel_necessario())
            } else {
                format!(
                    "Tier {}",
                    skills::tier_romano(self.progresso.tier(skill.id))
                )
            };
            estilo::texto(r.x + 12.0 * f, r.y + 55.0 * f, &estado, 13, estilo::SUAVE);
            if clique && r.contains(m) {
                self.selecionada = Some(skill.id);
            }
        }
        let Some(skill) = habilidades
            .iter()
            .copied()
            .find(|s| Some(s.id) == self.selecionada)
        else {
            return None;
        };
        let x = p.x + 328.0 * f;
        let y = p.y + 142.0 * f;
        let tier = self.progresso.tier(skill.id);
        estilo::texto_forte(x, y, &skill.nome, 20, estilo::OURO);
        estilo::texto_ajustado(
            skill.descricao(),
            x,
            y + 29.0 * f,
            p.x + p.w - x - 20.0 * f,
            13,
            estilo::TEXTO,
        );
        estilo::texto(
            x,
            y + 59.0 * f,
            &format!(
                "Tier {}  ·  poder {:.0}%",
                skills::tier_romano(tier),
                skills::multiplicador_do_tier(tier) * 100.0
            ),
            16,
            estilo::TEXTO,
        );
        for (i, marco) in [5u8, 8, 10].iter().enumerate() {
            let texto = format!(
                "{}: {}",
                skills::tier_romano(*marco),
                skills::despertar(skill.id, *marco)
            );
            estilo::texto(
                x,
                y + (92.0 + i as f32 * 28.0) * f,
                &texto,
                13,
                if tier >= *marco {
                    estilo::OURO
                } else {
                    estilo::SUAVE
                },
            );
        }
        let Some(custo) = skills::custo_de_evolucao(tier) else {
            estilo::texto(x, y + 212.0 * f, "Tier máximo alcançado.", 16, estilo::OURO);
            return None;
        };
        estilo::texto(
            x,
            y + 204.0 * f,
            &format!(
                "Próximo: Tier {} · nível {}",
                skills::tier_romano(custo.destino),
                custo.nivel
            ),
            15,
            estilo::TEXTO,
        );
        estilo::texto(
            x,
            y + 231.0 * f,
            &format!("Custo: {} Energia + {} cobre", custo.energia, custo.cobre),
            14,
            estilo::SUAVE,
        );
        let mut pode =
            nivel >= custo.nivel && self.progresso.energia >= custo.energia && cobre >= custo.cobre;
        if let Some(grau) = custo.tomo {
            let tomos = self.progresso.tomos(skill.id, grau);
            estilo::texto(
                x,
                y + 257.0 * f,
                &format!("Tomo {} desta habilidade: {}", grau.nome(), tomos),
                14,
                estilo::SUAVE,
            );
            pode &= tomos > 0;
            let custo_tomo = skills::custo_de_tomo(grau);
            let r = Rect::new(x, y + 291.0 * f, 210.0 * f, 43.0 * f);
            let fabrica = self.progresso.energia >= custo_tomo.energia && cobre >= custo_tomo.cobre;
            estilo::botao(
                r,
                "Condensar tomo",
                estilo::estado_de(r, !fabrica, false),
                fabrica,
            );
            estilo::texto(
                x + 218.0 * f,
                y + 317.0 * f,
                &format!("{} E + {} cobre", custo_tomo.energia, custo_tomo.cobre),
                12,
                estilo::SUAVE,
            );
            if clique && r.contains(m) && fabrica {
                return Some(ClientMessage::FabricarTomoDeSkill {
                    skill_id: skill.id,
                    grau: grau as u8,
                });
            }
        }
        let r = Rect::new(x, y + 352.0 * f, 225.0 * f, 48.0 * f);
        estilo::botao(r, "Evoluir", estilo::estado_de(r, !pode, false), pode);
        if clique && r.contains(m) && pode {
            return Some(ClientMessage::EvoluirSkill { skill_id: skill.id });
        }
        if let Some(aviso) = &self.aviso {
            estilo::texto(
                p.x + 20.0 * f,
                p.y + p.h - 20.0 * f,
                aviso,
                13,
                estilo::SUAVE,
            );
        }
        None
    }
}

/// Captura local do painel, sem conexão nem personagem real.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida = std::env::var("MMO_PREVIA_SAIDA")
        .unwrap_or_else(|_| "/tmp/tempest-evolucao-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut ui = EvolucaoSkills::default();
    ui.abrir();
    ui.progresso.energia = 35_000;
    ui.progresso.tiers[0] = 4;
    let catalogo = skills::playtest();
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha(&catalogo, 16, 9_000);
        unsafe { get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/evolucao.png"));
        next_frame().await;
    }
}
