//! The DAILIES panel, in the mould of MIR4's "Requests": separate from the
//! tracker and from the all-quests menu. Opens from the icon at the top right
//! and from the Menu — never by key (docs/HUD.md 2.5).
//!
//! Accepting and handing in are with the Quest Master: the server refuses
//! from far away. The "Take" button sends `AcceptQuest` directly since
//! 22/09/2026 — the old "Go accept" walked you to the Master (the offer opens
//! on arrival), "Go" switches on the auto quest and "Go hand in" too — it
//! already returns to the Master with the objective met.
use macroquad::prelude::*;
use shared::quests::{zona_da_missao, QuestDef, QUESTS};
use std::collections::HashMap;

use crate::hud_estilo::{self as estilo, u};
use crate::menu_missoes::{cadeado, clique_de, estado, reset_em, Clique, Contexto, Estado};

const LARGURA: f32 = 640.0;
const LINHA: f32 = 84.0;

/// As diarias da ilha `zona`, em ordem de id. Fora de ilha, nenhuma.
pub fn da_ilha(zona: Option<&str>) -> Vec<&'static QuestDef> {
    let Some(z) = zona else { return Vec::new() };
    let mut v: Vec<&'static QuestDef> = QUESTS
        .iter()
        .filter(|d| d.daily && zona_da_missao(d.id) == Some(z))
        .collect();
    v.sort_by_key(|d| d.id);
    v
}

/// A daily's state. Handed in and still before the reset is "completed today"
///  — in the general menu that showed as blocked with a cooldown in minutes.
pub fn estado_da_diaria(d: &QuestDef, c: &Contexto) -> Estado {
    let no_log = c.log.iter().any(|q| q.id == d.id);
    if !no_log && c.entregues.get(&d.id).is_some_and(|&cd| cd > c.agora_unix) {
        return Estado::Concluida;
    }
    estado(d, c)
}

/// The button label for each state; `None` = no button.
pub fn botao_de(e: &Estado) -> Option<&'static str> {
    match e {
        Estado::Disponivel => Some("Take"),
        Estado::EmAndamento { .. } => Some("Ir"),
        Estado::Pronta => Some("Go turn it in"),
        Estado::Concluida | Estado::Bloqueada(_) => None,
    }
}

/// The click on a daily. A completed one says when it comes back.
pub fn clique_da_diaria(d: &QuestDef, e: &Estado, agora_unix: i64) -> Clique {
    match e {
        Estado::Concluida => Clique::Aviso(format!(
            "\"{}\" completed today · resets in {}.",
            d.title,
            reset_em(agora_unix)
        )),
        _ => clique_de(d, e),
    }
}

/// Some daily to accept or hand in right now: a red dot.
pub fn tem_pendente(c: &Contexto) -> bool {
    da_ilha(c.zona)
        .iter()
        .any(|d| matches!(estado_da_diaria(d, c), Estado::Disponivel | Estado::Pronta))
}

/// "80 copper · 120 XP · 1x Experience Potion".
pub fn recompensa(d: &QuestDef, nomes: &HashMap<u16, String>) -> String {
    let mut partes = Vec::new();
    if d.reward_cobre > 0 {
        partes.push(format!("{} cobre", d.reward_cobre));
    }
    if d.reward_xp > 0 {
        partes.push(format!("{} XP", d.reward_xp));
    }
    for (id, qtd) in [
        (d.reward_item, d.reward_item_qty),
        (d.reward_item2, d.reward_item2_qty),
    ] {
        if id != 0 && qtd > 0 {
            let nome = nomes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("item {id}"));
            partes.push(format!("{qtd}× {nome}"));
        }
    }
    if partes.is_empty() {
        "—".into()
    } else {
        partes.join(" · ")
    }
}

#[derive(Default)]
pub struct Diarias {
    pub aberto: bool,
    rolagem: crate::rolagem::Rolagem,
    /// The magnifier on a collect-item daily: the item for "Where to get".
    pub onde_obter: Option<u16>,
}

impl Diarias {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.rolagem.zera();
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, 560.0)
    }

    fn painel() -> Rect {
        let k = Self::escala();
        let w = (LARGURA * k).min(screen_width() - 24.0);
        let h =
            (screen_height() - 140.0 * k).clamp((260.0 * k).min(screen_height() - 24.0), 720.0 * k);
        Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        )
    }

    /// Desenha o painel e devolve o clique do quadro.
    pub fn desenha(&mut self, c: &Contexto, nomes: &HashMap<u16, String>) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(c, nomes))
    }

    fn desenha_na_escala(&mut self, c: &Contexto, nomes: &HashMap<u16, String>) -> Option<Clique> {
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto(p.x + u(18.0), p.y + u(32.0), "Dailies", 22, estilo::OURO);
        let ilha = c
            .zona
            .and_then(shared::terreno::def_da_zona)
            .map_or("—", |d| d.nome);
        estilo::texto(
            p.x + u(18.0),
            p.y + u(54.0),
            &format!("{ilha} · resets in {}", reset_em(c.agora_unix)),
            14,
            estilo::SUAVE,
        );
        if crate::ui::botao(
            Rect::new(p.x + p.w - u(44.0), p.y + u(10.0), u(32.0), u(28.0)),
            "x",
            true,
        ) {
            self.aberto = false;
            return None;
        }
        let area = Rect::new(p.x + u(10.0), p.y + u(68.0), p.w - u(20.0), p.h - u(78.0));
        let mouse = Vec2::from(mouse_position());
        let lista = da_ilha(c.zona);
        let total = lista.len() as f32 * u(LINHA);
        // Toque na linha, no botao ou na lupa vale no SOLTAR (a lista rola
        // arrastando).
        let clique = self.rolagem.quadro(area, total, u(LINHA));
        let arrastando = self.rolagem.arrastando();
        let tocou = |r: Rect| clique.is_some_and(|c| r.contains(c) && area.contains(c));
        if lista.is_empty() {
            estilo::texto(
                area.x + u(8.0),
                area.y + u(22.0),
                "No daily on this island.",
                15,
                estilo::SUAVE,
            );
        }
        let mut saida = None;
        let mut dica: Option<Vec<String>> = None;
        crate::rolagem::recortar(Some(area));
        for (i, d) in lista.iter().enumerate() {
            let linha = Rect::new(
                area.x,
                area.y - self.rolagem.pos + i as f32 * u(LINHA),
                area.w - u(12.0),
                u(LINHA) - u(6.0),
            );
            if linha.y + linha.h < area.y || linha.y > area.y + area.h {
                continue;
            }
            let e = estado_da_diaria(d, c);
            let sobre = !arrastando && linha.contains(mouse) && area.contains(mouse);
            draw_rectangle(
                linha.x,
                linha.y,
                linha.w,
                linha.h,
                Color::new(1.0, 1.0, 1.0, if sobre { 0.08 } else { 0.03 }),
            );
            let vermelho = Color::new(0.85, 0.45, 0.40, 1.0);
            let (rotulo, cor) = match &e {
                Estado::Disponivel => ("Available".to_string(), Color::new(1.0, 0.84, 0.2, 1.0)),
                Estado::EmAndamento { feito, total } => {
                    (format!("In progress · {feito}/{total}"), estilo::TEXTO)
                }
                Estado::Pronta => ("Ready to turn in".to_string(), estilo::AUTO),
                Estado::Concluida => ("Completed today".to_string(), estilo::SUAVE),
                Estado::Bloqueada(m) => (m.first().cloned().unwrap_or_default(), vermelho),
            };
            let icone = vec2(linha.x + u(22.0), linha.y + u(26.0));
            match &e {
                Estado::Bloqueada(_) => cadeado(icone, u(9.0), vermelho),
                Estado::Concluida => {
                    draw_line(
                        icone.x - u(7.0),
                        icone.y,
                        icone.x - u(2.0),
                        icone.y + u(6.0),
                        u(3.0),
                        estilo::SUAVE,
                    );
                    draw_line(
                        icone.x - u(2.0),
                        icone.y + u(6.0),
                        icone.x + u(8.0),
                        icone.y - u(7.0),
                        u(3.0),
                        estilo::SUAVE,
                    );
                }
                Estado::Pronta => {
                    estilo::texto_centro(icone.x, icone.y + u(10.0), "?", 26, estilo::AUTO)
                }
                Estado::Disponivel => {
                    estilo::texto_centro(icone.x, icone.y + u(10.0), "!", 26, cor)
                }
                Estado::EmAndamento { .. } => {
                    draw_circle_lines(icone.x, icone.y, u(8.0), u(2.0), estilo::TEXTO)
                }
            }
            let apagada = matches!(e, Estado::Bloqueada(_) | Estado::Concluida);
            let largura_texto = linha.w - u(170.0);
            estilo::texto_ajustado(
                d.title,
                linha.x + u(44.0),
                linha.y + u(22.0),
                largura_texto,
                16,
                if apagada {
                    estilo::SUAVE
                } else {
                    estilo::TEXTO
                },
            );
            estilo::texto_ajustado(
                &rotulo,
                linha.x + u(44.0),
                linha.y + u(42.0),
                largura_texto,
                13,
                cor,
            );
            estilo::texto_ajustado(
                &recompensa(d, nomes),
                linha.x + u(44.0),
                linha.y + u(62.0),
                largura_texto,
                13,
                estilo::OURO,
            );
            if d.obj_kind == shared::quests::objective_kind::COLLECT
                && d.obj_target != 0
                && !apagada
            {
                let lupa = Rect::new(
                    linha.x + linha.w - u(164.0),
                    linha.y + u(20.0),
                    u(36.0),
                    u(34.0),
                );
                let _ = crate::onde_obter::botao(lupa);
                if tocou(lupa) {
                    self.onde_obter = Some(d.obj_target);
                }
            }
            if let Some(t) = botao_de(&e) {
                let b = Rect::new(
                    linha.x + linha.w - u(122.0),
                    linha.y + u(22.0),
                    u(110.0),
                    u(30.0),
                );
                let _ = crate::ui::botao(b, t, true);
                if tocou(b) {
                    saida = Some(clique_da_diaria(d, &e, c.agora_unix));
                }
            } else if tocou(linha) {
                saida = Some(clique_da_diaria(d, &e, c.agora_unix));
            }
            if let Estado::Bloqueada(m) = &e {
                if sobre {
                    dica = Some(m.clone());
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(m) = dica {
            let w = m
                .iter()
                .map(|s| estilo::medir(s, 14))
                .fold(160.0f32, f32::max)
                + u(24.0);
            let h = u(30.0) + m.len() as f32 * u(20.0);
            let x = (mouse.x + u(16.0)).min(screen_width() - w - u(8.0));
            let y = (mouse.y + u(12.0)).min(screen_height() - h - u(8.0));
            estilo::painel(Rect::new(x, y, w, h));
            cadeado(
                vec2(x + u(16.0), y + u(17.0)),
                u(7.0),
                Color::new(0.85, 0.45, 0.40, 1.0),
            );
            estilo::texto(x + u(30.0), y + u(22.0), "Prerequisites", 14, estilo::OURO);
            for (i, s) in m.iter().enumerate() {
                estilo::texto(
                    x + u(12.0),
                    y + u(42.0) + i as f32 * u(20.0),
                    s,
                    14,
                    estilo::TEXTO,
                );
            }
        }
        saida
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::quests::{faction_id, quest_by_id, quest_status, QuestNet};

    fn nada(_: u16) -> u32 {
        0
    }

    fn ctx<'a>(
        log: &'a [QuestNet],
        entregues: &'a HashMap<u16, i64>,
        zona: Option<&'a str>,
    ) -> Contexto<'a> {
        Contexto {
            nomes: &crate::menu_missoes::NOMES_DE_TESTE,
            log,
            entregues,
            nivel: 50,
            faccao: faction_id::PEACEMAIN,
            zona,
            agora_unix: 1_000,
            tem: &nada,
        }
    }

    #[test]
    fn so_as_diarias_da_ilha_em_ordem() {
        let ids: Vec<u16> = da_ilha(Some("ilha_inicial")).iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![601, 602, 603, 604, 605, 606, 607, 608]);
        assert!(da_ilha(None).is_empty(), "fora de ilha nao ha' diaria");
        let gelo = da_ilha(Some("ilha_gelo"));
        assert!(
            !gelo.is_empty()
                && gelo
                    .iter()
                    .all(|d| d.daily && zona_da_missao(d.id) == Some("ilha_gelo"))
        );
    }

    #[test]
    fn estados_e_botoes_levam_ao_mestre() {
        let vazio = HashMap::new();
        let d601 = quest_by_id(601).unwrap();
        let c = ctx(&[], &vazio, Some("ilha_inicial"));
        let e = estado_da_diaria(d601, &c);
        assert_eq!(e, Estado::Disponivel);
        assert_eq!(botao_de(&e), Some("Take"));
        assert_eq!(
            clique_da_diaria(d601, &e, 1_000),
            Clique::Aceitar(601),
            "pega na hora, sem andar ate' o Mestre"
        );

        // The Hunt (raid) does not exist yet; the dungeon (606) already counts.
        let d607 = quest_by_id(607).unwrap();
        assert_eq!(
            estado_da_diaria(d607, &c),
            Estado::Bloqueada(vec!["Coming soon".into()])
        );
        assert_eq!(botao_de(&estado_da_diaria(d607, &c)), None);
        assert_eq!(
            estado_da_diaria(quest_by_id(606).unwrap(), &c),
            Estado::Disponivel
        );

        let mut entregues = HashMap::new();
        entregues.insert(601, 50_000);
        let c = ctx(&[], &entregues, Some("ilha_inicial"));
        let e = estado_da_diaria(d601, &c);
        assert_eq!(
            e,
            Estado::Concluida,
            "entregue antes do reset: concluida hoje"
        );
        assert_eq!(botao_de(&e), None);
        assert!(
            matches!(clique_da_diaria(d601, &e, 1_000), Clique::Aviso(t) if t.contains("reset"))
        );

        let d602 = quest_by_id(602).unwrap();
        let mut q = QuestNet::from_def(d602, quest_status::ACTIVE, 4);
        let log = vec![q.clone()];
        let c = ctx(&log, &vazio, Some("ilha_inicial"));
        let e = estado_da_diaria(d602, &c);
        assert!(matches!(e, Estado::EmAndamento { feito: 4, .. }), "{e:?}");
        assert_eq!(botao_de(&e), Some("Ir"));
        assert_eq!(clique_da_diaria(d602, &e, 1_000), Clique::AutoMissao(602));
        q.status = quest_status::READY;
        let log = vec![q];
        let c = ctx(&log, &vazio, Some("ilha_inicial"));
        let e = estado_da_diaria(d602, &c);
        assert_eq!(e, Estado::Pronta);
        assert_eq!(botao_de(&e), Some("Go turn it in"));
        assert_eq!(
            clique_da_diaria(d602, &e, 1_000),
            Clique::AutoMissao(602),
            "a auto missao volta ao Mestre"
        );
    }

    #[test]
    fn ponto_vermelho_so_com_algo_pra_aceitar_ou_entregar() {
        let vazio = HashMap::new();
        assert!(tem_pendente(&ctx(&[], &vazio, Some("ilha_inicial"))));
        // Everything that exists handed in today; the "coming soon" ones stay locked.
        let entregues: HashMap<u16, i64> = da_ilha(Some("ilha_inicial"))
            .iter()
            .filter(|d| !d.em_breve)
            .map(|d| (d.id, 50_000))
            .collect();
        assert!(!tem_pendente(&ctx(&[], &entregues, Some("ilha_inicial"))));
        assert!(!tem_pendente(&ctx(&[], &vazio, None)));
    }

    #[test]
    fn recompensa_lista_cobre_xp_e_itens() {
        let d = quest_by_id(601).unwrap();
        let mut nomes = HashMap::new();
        nomes.insert(d.reward_item2, "Experience Potion".to_string());
        nomes.insert(d.reward_item, "Item".to_string());
        let r = recompensa(d, &nomes);
        if d.reward_cobre > 0 {
            assert!(r.contains("cobre"), "{r}");
        }
        if d.reward_item2 != 0 {
            assert!(r.contains("Experience Potion"), "{r}");
        }
    }
}
