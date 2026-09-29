//! Evolution of the twelve skills. The screen only presents costs and sends
//! requests; Energy, copper, tomes and tier are always checked by the server.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::skills::{self, Conjunto, GrauTomo, ProgressoDeSkills, Skill};

use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct EvolucaoSkills {
    pub aberto: bool,
    pub progresso: ProgressoDeSkills,
    conjunto: usize,
    selecionada: Option<u32>,
    tomo_selecionado: usize,
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

    /// Can SOME skill be evolved right now? It is the red dot on the Skills tab:
    /// without it, the player only found out they had gathered enough Energy and
    /// tomes by opening the panel and checking all twelve by hand.
    ///
    /// The same four sums as the "Evolve skill" button: level, Energy, copper
    /// and tome. There is no point lighting up for one while the button stays dark.
    pub fn pode_evoluir_alguma(&self, catalogo: &[Skill], nivel: u32, cobre: u32) -> bool {
        catalogo.iter().any(|skill| {
            if nivel < skill.nivel_necessario() {
                return false;
            }
            skills::custo_de_evolucao(self.progresso.tier(skill.id)).is_some_and(|c| {
                nivel >= c.nivel
                    && self.progresso.energia >= c.energia
                    && cobre >= c.cobre
                    && c.tomo.is_none_or(|g| self.progresso.tomos(skill.id, g) > 0)
            })
        })
    }

    pub fn desenha(&mut self, catalogo: &[Skill], nivel: u32, cobre: u32) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        let seguro = crate::hud_layout::tela_segura();
        let escala = estilo::escala_do_painel(900.0, 560.0)
            .min((seguro.w - 16.0) / 900.0)
            .min((seguro.h - 16.0) / 560.0);
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
        let w = (900.0 * f).min(seguro.w - 16.0);
        let h = (560.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.5);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = crate::foco::clique();
        estilo::texto_forte(
            p.x + 20.0 * f,
            p.y + 36.0 * f,
            "HABILIDADES",
            23,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(m) {
            self.fechar();
            return None;
        }
        estilo::icone_energia(vec2(p.x + 32.0 * f, p.y + 69.0 * f), 22.0 * f);
        estilo::texto(
            p.x + 50.0 * f,
            p.y + 75.0 * f,
            &format!("Energia {}", crate::bolsa::milhar(self.progresso.energia)),
            17,
            estilo::ACENTO,
        );
        estilo::texto(
            p.x + 265.0 * f,
            p.y + 75.0 * f,
            &format!("Cobre {}", crate::bolsa::milhar(cobre as u64)),
            16,
            estilo::OURO,
        );

        // ── o foco do tutorial cobre o CORPO do painel ──
        //
        // The step is "evolve a skill", not "evolve this one". Marking only the
        // SELECTED skill's button, the focus locked the touch on everything else and
        // the only one that could be raised was the first in the list — the same
        // defect the Sheet's "+" column had, found while playing on 20/09/2026.
        //
        // The WHOLE panel, and not only from the list down: the weapon-set tabs,
        // the twelve skills, the evolve button — and the X. Leaving the X out
        // trapped the player in the panel until the focus expired by itself.
        crate::foco::marca(crate::foco::chave::SKILL_EVOLUIR, p);

        let largura_aba = 210.0 * f;
        for (i, conjunto) in Conjunto::TODOS.iter().enumerate() {
            let r = Rect::new(
                p.x + (20.0 + i as f32 * 216.0) * f,
                p.y + 91.0 * f,
                largura_aba,
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
            let y = p.y + (144.0 + i as f32 * 101.0) * f;
            let r = Rect::new(p.x + 20.0 * f, y, 270.0 * f, 88.0 * f);
            estilo::cartao(r, r.contains(m), self.selecionada == Some(skill.id));
            let ic = vec2(r.x + 44.0 * f, r.y + 44.0 * f);
            if !crate::icones_ui::skill(skill.id, ic, 62.0 * f, 1.0) {
                estilo::icone(skill.id, ic, 25.0 * f, estilo::ACENTO);
            }
            estilo::texto_ajustado(
                &skill.nome,
                r.x + 82.0 * f,
                r.y + 34.0 * f,
                175.0 * f,
                16,
                estilo::TEXTO,
            );
            let estado = if nivel < skill.nivel_necessario() {
                format!("Libera no nível {}", skill.nivel_necessario())
            } else {
                format!(
                    "Tier {}  ·  +{:.0}%",
                    skills::tier_romano(self.progresso.tier(skill.id)),
                    (skills::multiplicador_do_tier(self.progresso.tier(skill.id)) - 1.0) * 100.0
                )
            };
            estilo::texto(r.x + 82.0 * f, r.y + 63.0 * f, &estado, 14, estilo::SUAVE);
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
        let x = p.x + 314.0 * f;
        let y = p.y + 144.0 * f;
        let tier = self.progresso.tier(skill.id);
        let ic = vec2(x + 36.0 * f, y + 36.0 * f);
        if !crate::icones_ui::skill(skill.id, ic, 68.0 * f, 1.0) {
            estilo::icone(skill.id, ic, 29.0 * f, estilo::ACENTO);
        }
        estilo::texto_ajustado(
            &skill.nome,
            x + 84.0 * f,
            y + 27.0 * f,
            475.0 * f,
            21,
            estilo::OURO,
        );
        estilo::texto(
            x + 84.0 * f,
            y + 56.0 * f,
            &format!(
                "Tier {}  ·  Poder {:.0}%",
                skills::tier_romano(tier),
                skills::multiplicador_do_tier(tier) * 100.0
            ),
            16,
            estilo::TEXTO,
        );
        estilo::texto_ajustado(
            skill.descricao(),
            x,
            y + 92.0 * f,
            555.0 * f,
            14,
            estilo::TEXTO,
        );
        for i in 0..skills::TIER_MAX {
            let r = Rect::new(x + i as f32 * 56.0 * f, y + 109.0 * f, 51.0 * f, 11.0 * f);
            let cor = if i < tier {
                estilo::ACENTO
            } else {
                estilo::FUNDO_ALTO
            };
            draw_rectangle(r.x, r.y, r.w, r.h, cor);
        }
        for (i, marco) in [5u8, 8, 10].iter().enumerate() {
            let texto = format!(
                "Tier {}: {}",
                skills::tier_romano(*marco),
                skills::despertar(skill.id, *marco)
            );
            estilo::texto_ajustado(
                &texto,
                x,
                y + (141.0 + i as f32 * 23.0) * f,
                555.0 * f,
                13,
                if tier >= *marco {
                    estilo::OURO
                } else {
                    estilo::SUAVE
                },
            );
        }
        // WHAT IS MISSING, item by item — and not everything on one grey line.
        //
        // It was: "Next: Tier V · lv 15 · 1,500 Energy + 2,000 copper + Green
        // tome". All true, and illegible: it did not say what the player HAS, so
        // they did not know what was missing nor how much they would spend. The
        // owner: "there's no way to know how many books you're spending, how many I
        // have, nothing there makes sense".
        //
        // One line per requirement, each with what is asked and what is held, green
        // when met and red when not. The eye finds the red.
        let custo = skills::custo_de_evolucao(tier);
        match custo {
            None => estilo::texto_ajustado(
                "Tier máximo alcançado",
                x,
                y + 215.0 * f,
                555.0 * f,
                14,
                estilo::OURO,
            ),
            Some(c) => {
                estilo::texto(
                    x,
                    y + 200.0 * f,
                    &format!("PARA O TIER {}", skills::tier_romano(c.destino)),
                    13,
                    estilo::OURO,
                );
                let tem_tomo = c
                    .tomo
                    .map_or(0, |g| self.progresso.tomos(skill.id, g) as u64);
                let linhas: [(String, bool); 4] = [
                    (
                        format!("Nível {} · você: {nivel}", c.nivel),
                        nivel >= c.nivel,
                    ),
                    (
                        format!(
                            "{} Energia · você: {}",
                            crate::bolsa::milhar(c.energia),
                            crate::bolsa::milhar(self.progresso.energia)
                        ),
                        self.progresso.energia >= c.energia,
                    ),
                    (
                        format!(
                            "{} cobre · você: {}",
                            crate::bolsa::milhar(c.cobre as u64),
                            crate::bolsa::milhar(cobre as u64)
                        ),
                        cobre >= c.cobre,
                    ),
                    match c.tomo {
                        Some(g) => (
                            format!("1 tomo {} · você: {tem_tomo}", g.nome()),
                            tem_tomo > 0,
                        ),
                        // Saying it does NOT need a tome matters as much as saying
                        // it does: the first three tiers do not, and the owner
                        // thought it was bugged for being able to evolve
                        // "without having books".
                        None => ("Este tier não pede tomo.".to_string(), true),
                    },
                ];
                for (i, (t, ok)) in linhas.iter().enumerate() {
                    estilo::texto(
                        x + (i % 2) as f32 * 285.0 * f,
                        y + (216.0 + (i / 2) as f32 * 19.0) * f,
                        t,
                        13,
                        if *ok { estilo::VERDE } else { estilo::VERMELHO },
                    );
                }
            }
        }
        estilo::texto(x, y + 258.0 * f, "TOMOS DESTA HABILIDADE", 15, estilo::OURO);

        for (i, grau) in GrauTomo::TODOS.iter().enumerate() {
            let r = Rect::new(x + i as f32 * 188.0 * f, y + 270.0 * f, 180.0 * f, 70.0 * f);
            estilo::cartao(r, r.contains(m), self.tomo_selecionado == i);
            let cor = match grau {
                GrauTomo::Verde => estilo::VERDE,
                GrauTomo::Roxo => Color::new(0.75, 0.52, 1.0, 1.0),
                GrauTomo::Lendario => estilo::OURO,
            };
            let c = vec2(r.x + 29.0 * f, r.y + 34.0 * f);
            if !crate::icones_ui::ui("habilidades", c, 42.0 * f, cor) {
                estilo::icone(skill.id, c, 18.0 * f, cor);
            }
            estilo::texto(r.x + 57.0 * f, r.y + 27.0 * f, grau.nome(), 14, cor);
            estilo::texto_forte(
                r.x + 57.0 * f,
                r.y + 52.0 * f,
                &format!("x{}", self.progresso.tomos(skill.id, *grau)),
                19,
                estilo::TEXTO,
            );
            if clique && r.contains(m) {
                self.tomo_selecionado = i;
            }
        }
        let grau = GrauTomo::TODOS[self.tomo_selecionado.min(2)];
        let custo_tomo = skills::custo_de_tomo(grau);
        estilo::texto(
            x,
            y + 339.0 * f,
            // "Condense" said nothing to anyone — the owner: "condense a book, I don't
            // even know what that is". What the button does is MAKE a tome for this
            // skill by spending Energy and copper; so that is what it says.
            &format!(
                "Fabricar 1 tomo {} desta habilidade — custa {} Energia + {} cobre",
                grau.nome(),
                crate::bolsa::milhar(custo_tomo.energia),
                crate::bolsa::milhar(custo_tomo.cobre as u64)
            ),
            13,
            estilo::SUAVE,
        );
        let fabrica = self.progresso.energia >= custo_tomo.energia && cobre >= custo_tomo.cobre;
        let fabricar = Rect::new(x, y + 357.0 * f, 260.0 * f, 45.0 * f);
        estilo::botao(
            fabricar,
            &format!("Fabricar tomo {}", grau.nome()),
            estilo::estado_de(fabricar, !fabrica, false),
            fabrica,
        );
        if clique && fabricar.contains(m) && fabrica {
            return Some(ClientMessage::FabricarTomoDeSkill {
                skill_id: skill.id,
                grau: grau as u8,
            });
        }
        let pode = custo.is_some_and(|c| {
            nivel >= c.nivel
                && self.progresso.energia >= c.energia
                && cobre >= c.cobre
                && c.tomo.is_none_or(|g| self.progresso.tomos(skill.id, g) > 0)
        });
        let evoluir = Rect::new(x + 287.0 * f, y + 357.0 * f, 267.0 * f, 45.0 * f);
        estilo::botao(
            evoluir,
            "Evoluir habilidade",
            estilo::estado_de(evoluir, !pode, false),
            pode,
        );
        if clique && evoluir.contains(m) && pode {
            return Some(ClientMessage::EvoluirSkill { skill_id: skill.id });
        }
        if let Some(aviso) = &self.aviso {
            estilo::texto_ajustado(
                aviso,
                p.x + 20.0 * f,
                p.y + p.h - 17.0 * f,
                p.w - 40.0 * f,
                13,
                estilo::SUAVE,
            );
        }
        None
    }
}

/// Local capture of the panel, with no connection and no real character.
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
    ui.progresso.tomos[0] = [3, 1, 0];
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
