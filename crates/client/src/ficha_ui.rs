//! Ficha do personagem: atributos alocados e números finais enviados pelo
//! servidor. Os botões só enviam pedidos; não calculam nem gastam pontos aqui.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::{PlayerStats, STAT_COUNT};

use crate::hud_estilo as estilo;

const ATRIBUTOS: [(&str, &str); STAT_COUNT] = [
    ("FOR", "+1 ataque · +2 vida"),
    ("DES", "+1 destreza · crítico/ataque"),
    ("INT", "+1 dano mágico · +2 mana"),
    ("VIT", "+5 vida · regeneração"),
    ("SPD", "+2 vigor · regeneração · recarga do Dash"),
    ("RES", "+1 defesa · bloqueio"),
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
}

impl FichaUi {
    pub fn abrir(&mut self) {
        self.aberta = true;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.confirmar_reset = false;
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
        self.confirmar_reset = false;
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
            self.desenha_na_escala(nome, nivel, xp, mult_xp, stats)
        })
    }

    fn desenha_na_escala(
        &mut self,
        nome: &str,
        nivel: u32,
        xp: u64,
        mult_xp: u64,
        stats: Option<&PlayerStats>,
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
            "FICHA DO PERSONAGEM",
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
            &format!("Nível {}", nivel.max(1)),
            15,
            estilo::SUAVE,
        );
        let poder = stats.map(crate::bolsa::poder);
        estilo::texto_forte(
            p.x + 545.0 * f,
            p.y + 75.0 * f,
            &format!(
                "PODER  {}",
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
            &format!("XP para o próximo nível: {}", crate::bolsa::milhar(falta)),
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
            "ATRIBUTOS",
            17,
            estilo::OURO,
        );
        let pontos = self.pontos_disponiveis();
        estilo::texto(
            esq.x + 120.0 * f,
            esq.y + 27.0 * f,
            &format!(
                "Livres: {}",
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
            "Energia: {} · ponto −{}",
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
                "Toque novamente para confirmar",
                11,
                estilo::SUAVE,
            );
        }
        estilo::botao(
            reset,
            if self.confirmar_reset {
                "Confirmar reset"
            } else {
                "Redistribuir grátis"
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

        estilo::texto_forte(
            dir.x + 14.0 * f,
            dir.y + 28.0 * f,
            "STATUS DE COMBATE",
            17,
            estilo::OURO,
        );
        if let Some(s) = stats {
            let linhas = [
                ("Vida máxima", s.hp_max.to_string()),
                ("Mana máxima", s.mp_max.to_string()),
                ("Ataque", s.attack_damage.to_string()),
                ("Defesa", s.defense.to_string()),
                ("Destreza", s.dex.to_string()),
                ("Sabedoria", s.wis.to_string()),
                ("Crítico", format!("{:.1}%", s.crit_chance * 100.0)),
                ("Vel. ataque", format!("{:.2}x", s.attack_speed_mult)),
                ("Reg. vida", format!("{:.1}/s", s.hp_regen)),
                ("Reg. mana", format!("{:.1}/s", s.mp_regen)),
                ("Vigor máx.", s.stamina_max.to_string()),
                ("Reg. vigor", format!("{:.1}/s", s.stamina_regen)),
                ("Bloqueio", format!("{:.0}%", s.block_dmg_reduction * 100.0)),
                (
                    "Red. dano",
                    format!("{:.0}%", s.damage_reduction_pct * 100.0),
                ),
                ("Recarga do Dash", format!("{:.2}x", s.dash_cd_mult)),
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
                "Aguardando atributos do servidor…",
                14,
                estilo::SUAVE,
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ui.desenha("Camteste", 16, (xp16 + xp17) / 2, 1, Some(&stats), 1_480);
        unsafe { get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/ficha.png"));
        next_frame().await;
    }
}
