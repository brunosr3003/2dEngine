//! Ficha do personagem: atributos alocados e números finais enviados pelo
//! servidor. Os botões só enviam pedidos; não calculam nem gastam pontos aqui.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::{PlayerStats, STAT_COUNT};

use crate::hud_estilo as estilo;

const ATRIBUTOS: [(&str, &str); STAT_COUNT] = [
    ("FOR", "+1 attack · +2 health"),
    ("DES", "+1 dexterity · crit/attack · pistol: +1 health, +0.5% dodge"),
    ("INT", "+1 magic damage · +2 mana · -0.1% skill cooldown"),
    ("VIT", "+5 health · regeneration"),
    ("SPD", "+2 vigor · regeneração · recarga do Dash"),
    ("RES", "+1 defence · block"),
];

#[derive(Default)]
pub struct FichaUi {
    /// O que o EQUIPAMENTO empresta por atributo (armadura media, pet,
    /// montaria). Vem separado do alocado — ver `atualizar_pontos`.
    emprestados: [u32; STAT_COUNT],
    pub aberta: bool,
    pontos: Option<(u32, [u32; STAT_COUNT])>,
    /// Saldo de Energia da evolucao: alocar atributo tambem gasta dela.
    energia: u64,
    confirmar_reset: bool,
    proficiencias: Option<[u64; shared::PROF_COUNT]>,
    aba_proficiencias: bool,
    prof_selecionada: Option<usize>,
}

impl FichaUi {
    pub fn abrir(&mut self) {
        self.aberta = true;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.confirmar_reset = false;
        self.prof_selecionada = None;
    }

    pub fn atualizar_pontos(
        &mut self,
        unspent: u32,
        allocated: [u32; STAT_COUNT],
        emprestados: [u32; STAT_COUNT],
    ) {
        self.pontos = Some((unspent, allocated));
        self.emprestados = emprestados;
        self.confirmar_reset = false;
    }

    /// O que o equipamento empresta de cada atributo (ver `atualizar_pontos`).
    pub fn limpar_pontos(&mut self) {
        self.pontos = None;
        self.proficiencias = None;
        self.prof_selecionada = None;
        self.confirmar_reset = false;
    }

    pub fn atualizar_proficiencias(&mut self, xp: [u64; shared::PROF_COUNT]) {
        self.proficiencias = Some(xp);
    }

    pub fn nivel_da_arma(&self, arma: Option<u16>) -> Option<u32> {
        let conjunto = shared::skills::Conjunto::da_arma(arma?);
        self.proficiencias
            .map(|xp| shared::proficiency_level(xp[conjunto as usize]))
    }

    pub fn pontos_disponiveis(&self) -> Option<u32> {
        self.pontos.map(|(n, _)| n)
    }

    /// Ha' ponto sobrando pra distribuir: vira o ponto vermelho da Ficha e do
    /// MENU. Sem snapshot do servidor nao ha' selo — nao inventamos pendencia.
    pub fn tem_ponto_sobrando(&self) -> bool {
        self.pontos_disponiveis().is_some_and(|n| n > 0)
    }

    /// Energia do proximo ponto. Sobe com o que ja' foi alocado, entao a
    /// conta e' a mesma do servidor — o botao nunca promete o que sera'
    /// recusado.
    pub fn custo_do_proximo_ponto(&self) -> u64 {
        let ja: u32 = self.pontos.map_or(0, |(_, a)| a.iter().sum());
        shared::custo_energia_do_ponto(ja)
    }

    fn pode_alocar(&self, indice: usize) -> bool {
        indice < STAT_COUNT
            && self.pontos.is_some_and(|(n, _)| n > 0)
            && self.energia >= self.custo_do_proximo_ponto()
    }

    pub fn desenha(
        &mut self,
        nome: &str,
        nivel: u32,
        xp: u64,
        mult_xp: u64,
        stats: Option<&PlayerStats>,
        energia: u64,
        arma_equipada: Option<u16>,
    ) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        self.energia = energia;
        let seguro = crate::hud_layout::tela_segura();
        let escala = estilo::escala_do_painel(860.0, 540.0)
            .min((seguro.w - 16.0) / 860.0)
            .min((seguro.h - 16.0) / 540.0);
        estilo::no_painel(escala, || {
            self.desenha_na_escala(nome, nivel, xp, mult_xp, stats, arma_equipada)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nome: &str,
        nivel: u32,
        xp: u64,
        mult_xp: u64,
        stats: Option<&PlayerStats>,
        arma_equipada: Option<u16>,
    ) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (860.0 * f).min(seguro.w - 16.0);
        let h = (540.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        let mouse = Vec2::from(mouse_position());
        let clique = crate::foco::clique();

        estilo::texto_forte(
            p.x + 20.0 * f,
            p.y + 36.0 * f,
            "CHARACTER SHEET",
            23,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(mouse) {
            self.fechar();
            return None;
        }

        estilo::texto_ajustado(
            nome,
            p.x + 22.0 * f,
            p.y + 70.0 * f,
            405.0 * f,
            19,
            estilo::TEXTO,
        );
        estilo::texto(
            p.x + 22.0 * f,
            p.y + 97.0 * f,
            &format!("Level {}", nivel.max(1)),
            15,
            estilo::SUAVE,
        );
        let poder = stats.map(crate::bolsa::poder);
        estilo::texto_forte(
            p.x + 545.0 * f,
            p.y + 75.0 * f,
            &format!(
                "POWER  {}",
                poder.map_or_else(|| "—".into(), |v| crate::bolsa::milhar(v.max(0) as u64))
            ),
            19,
            estilo::OURO,
        );

        let base = shared::xp_for_level_with_mult(nivel.max(1), mult_xp.max(1));
        let prox = shared::xp_for_level_with_mult(nivel.max(1).saturating_add(1), mult_xp.max(1));
        let falta = prox.saturating_sub(xp);
        let frac = if prox > base {
            (xp.saturating_sub(base) as f32 / (prox - base) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        estilo::barra(
            Rect::new(p.x + 22.0 * f, p.y + 112.0 * f, 816.0 * f, 15.0 * f),
            frac,
            frac,
            estilo::ACENTO,
            None,
        );
        estilo::texto(
            p.x + 22.0 * f,
            p.y + 143.0 * f,
            &format!("XP to the next level: {}", crate::bolsa::milhar(falta)),
            13,
            estilo::SUAVE,
        );

        let esq = Rect::new(p.x + 18.0 * f, p.y + 155.0 * f, 401.0 * f, 366.0 * f);
        let dir = Rect::new(p.x + 429.0 * f, p.y + 155.0 * f, 413.0 * f, 366.0 * f);
        estilo::cartao(esq, false, false);
        estilo::cartao(dir, false, false);
        estilo::texto_forte(
            esq.x + 14.0 * f,
            esq.y + 28.0 * f,
            "ATTRIBUTES",
            17,
            estilo::OURO,
        );
        let pontos = self.pontos_disponiveis();
        estilo::texto(
            esq.x + 120.0 * f,
            esq.y + 27.0 * f,
            &format!(
                "Unspent: {}",
                pontos.map_or_else(|| "…".into(), |n| n.to_string())
            ),
            14,
            if pontos.unwrap_or(0) > 0 {
                estilo::VERDE
            } else {
                estilo::SUAVE
            },
        );
        // O ponto tambem custa Energia, e o preco sobe a cada ponto alocado:
        // o saldo fica ao lado do custo pra escolha nao virar tentativa.
        let custo = self.custo_do_proximo_ponto();
        let paga = self.energia >= custo;
        let texto_energia = format!(
            "Energy: {} · point −{}",
            crate::bolsa::milhar(self.energia),
            crate::bolsa::milhar(custo)
        );
        estilo::texto(
            esq.x + esq.w - 14.0 * f - estilo::medir(&texto_energia, 13),
            esq.y + 27.0 * f,
            &texto_energia,
            13,
            if paga {
                estilo::SUAVE
            } else {
                estilo::VERMELHO
            },
        );

        // A COLUNA inteira dos "+", e nao um botao so': o passo e' "gaste um
        // ponto", nao "gaste em FOR". Marcando o primeiro que podia, o foco
        // do tutorial travava o toque em todos os outros e so' dava pra subir
        // forca — o dono achou jogando, em 20/09/2026.
        let mut coluna: Option<Rect> = None;
        for (i, (sigla, bonus)) in ATRIBUTOS.iter().enumerate() {
            let y = esq.y + (45.0 + i as f32 * 48.0) * f;
            let r = Rect::new(esq.x + 10.0 * f, y, esq.w - 20.0 * f, 45.0 * f);
            estilo::cartao(r, false, false);
            estilo::texto_forte(r.x + 10.0 * f, r.y + 18.0 * f, sigla, 16, estilo::TEXTO);
            estilo::texto_ajustado(
                bonus,
                r.x + 62.0 * f,
                r.y + 17.0 * f,
                208.0 * f,
                12,
                estilo::SUAVE,
            );
            let valor = self
                .pontos
                .map_or_else(|| "—".to_string(), |(_, a)| a[i].to_string());
            estilo::texto_forte(r.x + 283.0 * f, r.y + 29.0 * f, &valor, 20, estilo::OURO);
            // O QUE O EQUIPAMENTO EMPRESTA, ao lado e em verde.
            //
            // Separado do que se gastou, e nao somado: somar faria o jogador
            // achar que gastou pontos que nao gastou, e tirar a montaria
            // pareceria perder pontos. Sem mostrar, o emprestimo virava
            // invisivel — a armadura media dava FOR e ninguem via.
            let emp = self.emprestados[i];
            if emp > 0 {
                estilo::texto_forte(
                    r.x + 310.0 * f,
                    r.y + 29.0 * f,
                    &format!("+{emp}"),
                    15,
                    estilo::VERDE,
                );
            }
            let botao = Rect::new(r.x + r.w - 43.0 * f, r.y + 4.0 * f, 36.0 * f, 36.0 * f);
            let pode = self.pode_alocar(i);
            if pode {
                // So' os que dao pra apertar entram: apontar um travado seria
                // mandar o jogador bater num botao que nao responde.
                coluna = Some(match coluna {
                    None => botao,
                    Some(c) => Rect::new(
                        c.x.min(botao.x),
                        c.y.min(botao.y),
                        c.w.max(botao.x + botao.w - c.x.min(botao.x)),
                        (botao.y + botao.h).max(c.y + c.h) - c.y.min(botao.y),
                    ),
                });
            }
            estilo::botao(botao, "+", estilo::estado_de(botao, !pode, false), pode);
            if clique && botao.contains(mouse) && pode {
                return Some(ClientMessage::AllocStatPoint { stat: i as u8 });
            }
        }
        if let Some(c) = coluna {
            // O X entra no buraco junto: sem ele o jogador ficava preso no
            // painel ate' o foco expirar sozinho.
            let x0 = c.x.min(fechar.x);
            let y0 = c.y.min(fechar.y);
            crate::foco::marca(
                crate::foco::chave::FICHA_MAIS,
                Rect::new(
                    x0,
                    y0,
                    (c.x + c.w).max(fechar.x + fechar.w) - x0,
                    (c.y + c.h).max(fechar.y + fechar.h) - y0,
                ),
            );
        }
        let total: u32 = self.pontos.map_or(0, |(_, a)| a.iter().sum());
        let reset = Rect::new(esq.x + 10.0 * f, esq.y + 339.0 * f, 185.0 * f, 24.0 * f);
        if self.confirmar_reset {
            estilo::texto(
                esq.x + 201.0 * f,
                esq.y + 354.0 * f,
                "Tap again to confirm",
                11,
                estilo::SUAVE,
            );
        }
        estilo::botao(
            reset,
            if self.confirmar_reset {
                "Confirm reset"
            } else {
                "Redistribute for free"
            },
            estilo::estado_de(reset, total == 0, false),
            false,
        );
        if clique && reset.contains(mouse) && total > 0 {
            if self.confirmar_reset {
                self.confirmar_reset = false;
                return Some(ClientMessage::ResetStats);
            }
            self.confirmar_reset = true;
        } else if clique && self.confirmar_reset {
            self.confirmar_reset = false;
        }

        let aba_status = Rect::new(dir.x + 10.0 * f, dir.y + 7.0 * f, 190.0 * f, 34.0 * f);
        let aba_profs = Rect::new(dir.x + 205.0 * f, dir.y + 7.0 * f, 198.0 * f, 34.0 * f);
        estilo::botao(
            aba_status,
            "COMBAT",
            estilo::estado_de(aba_status, false, !self.aba_proficiencias),
            false,
        );
        estilo::botao(
            aba_profs,
            "PROFICIENCIES",
            estilo::estado_de(aba_profs, false, self.aba_proficiencias),
            false,
        );
        if clique && aba_status.contains(mouse) {
            self.aba_proficiencias = false;
        }
        if clique && aba_profs.contains(mouse) {
            self.aba_proficiencias = true;
        }
        if self.aba_proficiencias {
            self.desenha_proficiencias(dir, f, clique, mouse, arma_equipada);
        } else if let Some(s) = stats {
            let linhas = [
                ("Maximum health", s.hp_max.to_string()),
                ("Maximum mana", s.mp_max.to_string()),
                ("Attack", s.attack_damage.to_string()),
                ("Defence", s.defense.to_string()),
                ("Dexterity", s.dex.to_string()),
                ("Wisdom", s.wis.to_string()),
                ("Critical", format!("{:.1}%", s.crit_chance * 100.0)),
                ("Atk. speed", format!("{:.2}x", s.attack_speed_mult)),
                ("Health regen", format!("{:.1}/s", s.hp_regen)),
                ("Mana regen", format!("{:.1}/s", s.mp_regen)),
                ("Max stamina", s.stamina_max.to_string()),
                ("Stamina regen", format!("{:.1}/s", s.stamina_regen)),
                ("Block", format!("{:.0}%", s.block_dmg_reduction * 100.0)),
                (
                    "Dmg. red.",
                    format!("{:.0}%", s.damage_reduction_pct * 100.0),
                ),
                ("Dash cooldown", format!("{:.2}x", s.dash_cd_mult)),
            ];
            for (i, (rotulo, valor)) in linhas.iter().enumerate() {
                let col = i / 7;
                let row = i % 7;
                let x = dir.x + (14.0 + col as f32 * 199.0) * f;
                let y = dir.y + (62.0 + row as f32 * 42.0) * f;
                estilo::texto(x, y, rotulo, 12, estilo::SUAVE);
                estilo::texto_forte(x, y + 20.0 * f, valor, 17, estilo::TEXTO);
            }
        } else {
            estilo::texto(
                dir.x + 14.0 * f,
                dir.y + 65.0 * f,
                "Waiting for attributes from the server…",
                14,
                estilo::SUAVE,
            );
        }
        None
    }

    fn desenha_proficiencias(
        &mut self, dir: Rect, f: f32, clique: bool, mouse: Vec2, arma_equipada: Option<u16>,
    ) {
        const NOMES: [&str; shared::PROF_COUNT] =
            ["Sword and shield", "Katana", "Pistols", "Magic ring"];
        const ARMAS: [u16; shared::PROF_COUNT] = [
            shared::item_id::ESPADA_E_ESCUDO,
            shared::item_id::KATANA,
            shared::item_id::PISTOLAS,
            shared::item_id::ANEL_MAGICO,
        ];
        if let Some(i) = self.prof_selecionada {
            let voltar = Rect::new(dir.x + 10.0 * f, dir.y + 51.0 * f, 92.0 * f, 34.0 * f);
            estilo::botao(voltar, "BACK", estilo::estado_de(voltar, false, false), false);
            if clique && voltar.contains(mouse) {
                self.prof_selecionada = None;
                return;
            }
            let xp = self.proficiencias.map_or(0, |v| v[i]);
            let nivel = shared::proficiency_level(xp);
            estilo::texto_forte(dir.x + 116.0 * f, dir.y + 76.0 * f,
                &format!("{}  ·  Lv. {nivel}", NOMES[i]), 18, estilo::OURO);
            let ativo = arma_equipada == Some(ARMAS[i]);
            estilo::texto(dir.x + 14.0 * f, dir.y + 111.0 * f,
                if ativo { "Bonuses active with the equipped weapon" } else { "Equip this weapon to activate the bonuses" },
                14, if ativo { estilo::VERDE } else { estilo::SUAVE });
            let b = bonus_proficiencia(ARMAS[i], nivel);
            let bonus = [
                ("Maximum health", b.0), ("Maximum mana", b.1), ("Attack", b.2),
                ("Defence", b.3), ("Dexterity", b.4), ("Wisdom", b.5),
            ];
            let mut linha = 0;
            for (rotulo, valor) in bonus {
                if valor == 0 { continue; }
                let y = dir.y + (145.0 + linha as f32 * 45.0) * f;
                estilo::texto(dir.x + 14.0 * f, y, rotulo, 15, estilo::TEXTO);
                estilo::texto_forte(dir.x + 250.0 * f, y,
                    &format!("+{valor}"), 17, if ativo { estilo::VERDE } else { estilo::SUAVE });
                linha += 1;
            }
            estilo::texto(dir.x + 14.0 * f, dir.y + 281.0 * f,
                "Os bônus são calculados pelo seu nível atual.", 12, estilo::SUAVE);
            if i == 1 || i == 2 {
                estilo::texto(dir.x + 14.0 * f, dir.y + 304.0 * f,
                    "Destreza também aumenta o dano desta arma.", 12, estilo::SUAVE);
            }
            return;
        }
        for (i, nome) in NOMES.iter().enumerate() {
            let y = dir.y + (48.0 + i as f32 * 77.0) * f;
            let linha = Rect::new(dir.x + 10.0 * f, y, dir.w - 20.0 * f, 70.0 * f);
            estilo::cartao(linha, false, false);
            if clique && linha.contains(mouse) && self.proficiencias.is_some() {
                self.prof_selecionada = Some(i);
            }
            let Some(xp) = self.proficiencias.map(|v| v[i]) else {
                estilo::texto(
                    linha.x + 12.0 * f,
                    linha.y + 28.0 * f,
                    nome,
                    15,
                    estilo::TEXTO,
                );
                estilo::texto(
                    linha.x + 12.0 * f,
                    linha.y + 51.0 * f,
                    "Waiting for the server…",
                    12,
                    estilo::SUAVE,
                );
                continue;
            };
            let nivel = shared::proficiency_level(xp);
            estilo::texto_forte(
                linha.x + 12.0 * f,
                linha.y + 26.0 * f,
                nome,
                16,
                estilo::TEXTO,
            );
            estilo::texto_forte(
                linha.x + linha.w - 98.0 * f,
                linha.y + 26.0 * f,
                &format!("Lv. {nivel}"),
                16,
                estilo::OURO,
            );
            if nivel >= shared::PROFICIENCY_LEVEL_CAP {
                estilo::texto(
                    linha.x + 12.0 * f,
                    linha.y + 52.0 * f,
                    "Maximum level",
                    12,
                    estilo::VERDE,
                );
                continue;
            }
            let base = shared::proficiency_xp_for_level(nivel);
            let precisa = shared::proficiency_xp_to_next(nivel);
            let atual = xp.saturating_sub(base).min(precisa);
            estilo::barra(
                Rect::new(
                    linha.x + 12.0 * f,
                    linha.y + 39.0 * f,
                    linha.w - 145.0 * f,
                    11.0 * f,
                ),
                atual as f32 / precisa as f32,
                atual as f32 / precisa as f32,
                estilo::ACENTO,
                None,
            );
            estilo::texto(
                linha.x + linha.w - 125.0 * f,
                linha.y + 51.0 * f,
                &format!("{} / {} XP", atual, precisa),
                12,
                estilo::SUAVE,
            );
        }
    }
}

/// Mesma multiplicação e truncamento usados pelo servidor ao montar os atributos.
fn bonus_proficiencia(arma: u16, nivel: u32) -> (i32, i32, i32, i32, i32, i32) {
    let b = shared::weapon_scaling(arma);
    let n = nivel as f32;
    ((b.hp_max * n) as i32, (b.mp_max * n) as i32,
     (b.attack_damage * n) as i32, (b.defense * n) as i32,
     (b.dex * n) as i32, (b.wis * n) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_da_ficha_usa_o_mesmo_truncamento_do_combate() {
        assert_eq!(bonus_proficiencia(shared::item_id::PISTOLAS, 52), (0, 0, 5, 0, 26, 0));
        assert_eq!(bonus_proficiencia(shared::item_id::ESPADA_E_ESCUDO, 3), (3, 0, 0, 0, 0, 0));
    }

    #[test]
    fn todos_os_seis_atributos_recebem_pontos_do_servidor() {
        assert_eq!(ATRIBUTOS.len(), shared::STAT_COUNT);
        let mut ui = FichaUi::default();
        ui.energia = u64::MAX; // aqui o assunto e' o ponto, nao a Energia
        assert!(!ui.pode_alocar(0), "sem snapshot nao pode gastar");
        ui.atualizar_pontos(2, [1, 2, 3, 4, 5, 6], [0; STAT_COUNT]);
        assert_eq!(ui.pontos_disponiveis(), Some(2));
        assert!((0..shared::STAT_COUNT).all(|i| ui.pode_alocar(i)));
        assert!(!ui.pode_alocar(shared::STAT_COUNT));
        ui.atualizar_pontos(0, [1, 2, 3, 4, 5, 6], [0; STAT_COUNT]);
        assert!(!ui.pode_alocar(0));
    }

    /// O foco do tutorial tem que caber a COLUNA inteira dos "+": o passo e'
    /// "gaste um ponto", nao "gaste em FOR". Marcando so' o primeiro, o foco
    /// travava o toque nos outros cinco e o unico atributo que dava pra subir
    /// era forca.
    #[test]
    fn o_foco_do_tutorial_abre_a_coluna_toda_dos_mais() {
        // A uniao dos retangulos, como `desenha` monta: a mesma conta.
        let une = |a: Option<Rect>, b: Rect| -> Option<Rect> {
            Some(match a {
                None => b,
                Some(c) => Rect::new(
                    c.x.min(b.x),
                    c.y.min(b.y),
                    c.w.max(b.x + b.w - c.x.min(b.x)),
                    (b.y + b.h).max(c.y + c.h) - c.y.min(b.y),
                ),
            })
        };
        // A coluna do painel: um botao por atributo, 48 de passo.
        let botoes: Vec<Rect> = (0..STAT_COUNT)
            .map(|i| Rect::new(300.0, 45.0 + i as f32 * 48.0, 36.0, 36.0))
            .collect();
        let coluna = botoes.iter().fold(None, |a, b| une(a, *b)).expect("coluna");
        for (i, b) in botoes.iter().enumerate() {
            assert!(
                coluna.contains(b.center()),
                "o atributo {i} ficou de fora do foco"
            );
        }
        assert!(coluna.h >= botoes[STAT_COUNT - 1].y + 36.0 - botoes[0].y);

        // E so' os que DAO pra apertar entram: apontar um botao travado e'
        // mandar o jogador bater onde nao responde.
        let mut ui = FichaUi::default();
        ui.atualizar_pontos(1, [0; STAT_COUNT], [0; STAT_COUNT]);
        ui.energia = 9; // um a menos que o primeiro ponto custa
        assert!((0..STAT_COUNT).all(|i| !ui.pode_alocar(i)));
        let so_os_que_podem: Option<Rect> = botoes
            .iter()
            .enumerate()
            .filter(|(i, _)| ui.pode_alocar(*i))
            .fold(None, |a, (_, b)| une(a, *b));
        assert!(
            so_os_que_podem.is_none(),
            "sem Energia nenhum + entra no foco"
        );

        // E o X tem que caber no buraco: sem ele o jogador fica preso no
        // painel ate' o foco expirar sozinho.
        let fechar = Rect::new(600.0, 8.0, 36.0, 36.0);
        let com_x = une(Some(coluna), fechar).expect("uniao");
        assert!(com_x.contains(fechar.center()), "o X ficou de fora");
        for (i, b) in botoes.iter().enumerate() {
            assert!(com_x.contains(b.center()), "o atributo {i} saiu do buraco");
        }
    }

    #[test]
    fn ponto_vermelho_so_com_ponto_sobrando() {
        let mut ui = FichaUi::default();
        assert!(!ui.tem_ponto_sobrando(), "sem snapshot nao ha' pendencia");
        ui.atualizar_pontos(3, [0; STAT_COUNT], [0; STAT_COUNT]);
        assert!(ui.tem_ponto_sobrando());
        ui.atualizar_pontos(0, [1, 1, 1, 0, 0, 0], [0; STAT_COUNT]);
        assert!(!ui.tem_ponto_sobrando(), "gastou tudo: o selo some");
        ui.limpar_pontos();
        assert!(!ui.tem_ponto_sobrando());
    }

    #[test]
    fn sem_energia_o_botao_nao_promete_ponto() {
        let mut ui = FichaUi::default();
        ui.atualizar_pontos(5, [0; STAT_COUNT], [0; STAT_COUNT]);
        assert_eq!(ui.custo_do_proximo_ponto(), 10);
        ui.energia = 9;
        assert!(
            (0..STAT_COUNT).all(|i| !ui.pode_alocar(i)),
            "faltando 1 de Energia nenhum atributo aceita"
        );
        ui.energia = 10;
        assert!((0..STAT_COUNT).all(|i| ui.pode_alocar(i)));
        // O preco acompanha o que ja' esta' alocado, igual ao servidor.
        ui.atualizar_pontos(5, [4, 3, 2, 1, 0, 0], [0; STAT_COUNT]);
        assert_eq!(ui.custo_do_proximo_ponto(), 10 + 5 * 10);
        assert!(
            (0..STAT_COUNT).all(|i| !ui.pode_alocar(i)),
            "10 nao paga 60"
        );
        ui.energia = 60;
        assert!((0..STAT_COUNT).all(|i| ui.pode_alocar(i)));
        // Sem ponto livre nao adianta ter Energia.
        ui.atualizar_pontos(0, [4, 3, 2, 1, 0, 0], [0; STAT_COUNT]);
        assert!((0..STAT_COUNT).all(|i| !ui.pode_alocar(i)));
    }
}

/// Captura local sem rede, para conferir layout e legibilidade no celular.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-ficha-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut ui = FichaUi::default();
    ui.atualizar_pontos(7, [8, 13, 5, 11, 4, 2], [3, 0, 0, 2, 0, 0]);
    ui.atualizar_proficiencias([224, 1720, 67665, 2619]);
    ui.aba_proficiencias = true;
    ui.abrir();
    let mut stats = shared::base_player_stats();
    stats.hp_max = 278;
    stats.mp_max = 94;
    stats.attack_damage = 67;
    stats.defense = 18;
    stats.crit_chance = 0.12;
    let xp16 = shared::xp_for_level_with_mult(16, 1);
    let xp17 = shared::xp_for_level_with_mult(17, 1);
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha("Camteste", 16, (xp16 + xp17) / 2, 1, Some(&stats), 1_480,
            Some(shared::item_id::PISTOLAS));
        unsafe { get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/ficha.png"));
        next_frame().await;
    }
    ui.prof_selecionada = Some(2);
    crate::render3d::camera_padrao();
    clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
    ui.desenha("Camteste", 16, (xp16 + xp17) / 2, 1, Some(&stats), 1_480,
        Some(shared::item_id::PISTOLAS));
    unsafe { get_internal_gl().flush() };
    rt.texture.get_texture_data().export_png(&format!("{saida}/ficha-detalhe.png"));
}
