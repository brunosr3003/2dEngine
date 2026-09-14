//! O painel das DIARIAS, no molde dos "Pedidos" do MIR4: separado do
//! rastreador e do menu de todas as missoes. Abre pelo icone no topo direito
//! e pelo Menu — nunca por tecla (docs/HUD.md 2.5).
//!
//! Aceitar e entregar sao com o Mestre de Missoes: o servidor recusa longe
//! dele. Por isso nenhum botao manda `AcceptQuest` direto: "Ir aceitar" leva
//! ate' o Mestre (a oferta abre ao chegar), "Ir" liga a auto missao e "Ir
//! entregar" tambem — ela ja' volta ao Mestre com o objetivo cumprido.
use macroquad::prelude::*;
use shared::quests::{zona_da_missao, QuestDef, QUESTS};
use std::collections::HashMap;

use crate::hud_estilo as estilo;
use crate::menu_missoes::{cadeado, clique_de, estado, reset_em, Clique, Contexto, Estado};

const LARGURA: f32 = 640.0;
const LINHA: f32 = 84.0;

/// As diarias da ilha `zona`, em ordem de id. Fora de ilha, nenhuma.
pub fn da_ilha(zona: Option<&str>) -> Vec<&'static QuestDef> {
    let Some(z) = zona else { return Vec::new() };
    let mut v: Vec<&'static QuestDef> =
        QUESTS.iter().filter(|d| d.daily && zona_da_missao(d.id) == Some(z)).collect();
    v.sort_by_key(|d| d.id);
    v
}

/// Estado de uma diaria. Entregue e ainda antes do reset e' "concluida hoje"
/// — no menu geral isso aparecia como bloqueada com cooldown em minutos.
pub fn estado_da_diaria(d: &QuestDef, c: &Contexto) -> Estado {
    let no_log = c.log.iter().any(|q| q.id == d.id);
    if !no_log && c.entregues.get(&d.id).is_some_and(|&cd| cd > c.agora_unix) {
        return Estado::Concluida;
    }
    estado(d, c)
}

/// O rotulo do botao pra cada estado; `None` = sem botao.
pub fn botao_de(e: &Estado) -> Option<&'static str> {
    match e {
        Estado::Disponivel => Some("Ir aceitar"),
        Estado::EmAndamento { .. } => Some("Ir"),
        Estado::Pronta => Some("Ir entregar"),
        Estado::Concluida | Estado::Bloqueada(_) => None,
    }
}

/// O clique numa diaria. Concluida avisa quando volta.
pub fn clique_da_diaria(d: &QuestDef, e: &Estado, agora_unix: i64) -> Clique {
    match e {
        Estado::Concluida => Clique::Aviso(format!("\"{}\" concluída hoje · reset em {}.", d.title, reset_em(agora_unix))),
        _ => clique_de(d, e),
    }
}

/// Alguma diaria pra aceitar ou entregar agora: ponto vermelho.
pub fn tem_pendente(c: &Contexto) -> bool {
    da_ilha(c.zona).iter().any(|d| matches!(estado_da_diaria(d, c), Estado::Disponivel | Estado::Pronta))
}

/// "80 ouro · 120 XP · 1× Poção de Experiência".
pub fn recompensa(d: &QuestDef, nomes: &HashMap<u16, String>) -> String {
    let mut partes = Vec::new();
    if d.reward_gold > 0 {
        partes.push(format!("{} ouro", d.reward_gold));
    }
    if d.reward_xp > 0 {
        partes.push(format!("{} XP", d.reward_xp));
    }
    for (id, qtd) in [(d.reward_item, d.reward_item_qty), (d.reward_item2, d.reward_item2_qty)] {
        if id != 0 && qtd > 0 {
            let nome = nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"));
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
    rolagem: f32,
}

impl Diarias {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.rolagem = 0.0;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn painel() -> Rect {
        let w = LARGURA.min(screen_width() - 24.0);
        let h = (screen_height() - 140.0).clamp(260.0, 720.0);
        Rect::new((screen_width() - w) * 0.5, (screen_height() - h) * 0.5, w, h)
    }

    /// Desenha o painel e devolve o clique do quadro.
    pub fn desenha(&mut self, c: &Contexto, nomes: &HashMap<u16, String>) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto(p.x + 18.0, p.y + 32.0, "Diárias", 22, estilo::OURO);
        let ilha = c.zona.and_then(shared::terreno::def_da_zona).map_or("—", |d| d.nome);
        estilo::texto(p.x + 18.0, p.y + 54.0, &format!("{ilha} · reset em {}", reset_em(c.agora_unix)), 14, estilo::SUAVE);
        if crate::ui::botao(Rect::new(p.x + p.w - 44.0, p.y + 10.0, 32.0, 28.0), "x", true) {
            self.aberto = false;
            return None;
        }
        let area = Rect::new(p.x + 10.0, p.y + 68.0, p.w - 20.0, p.h - 78.0);
        let mouse = Vec2::from(mouse_position());
        let lista = da_ilha(c.zona);
        let total = lista.len() as f32 * LINHA;
        if area.contains(mouse) {
            let (_, roda) = mouse_wheel();
            self.rolagem = (self.rolagem - roda.signum() * LINHA).clamp(0.0, (total - area.h).max(0.0));
        }
        if lista.is_empty() {
            estilo::texto(area.x + 8.0, area.y + 22.0, "Nenhuma diária nesta ilha.", 15, estilo::SUAVE);
        }
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        let mut saida = None;
        let mut dica: Option<Vec<String>> = None;
        for (i, d) in lista.iter().enumerate() {
            let linha = Rect::new(area.x, area.y - self.rolagem + i as f32 * LINHA, area.w, LINHA - 6.0);
            if linha.y + linha.h < area.y || linha.y > area.y + area.h {
                continue;
            }
            let e = estado_da_diaria(d, c);
            let sobre = linha.contains(mouse) && area.contains(mouse);
            draw_rectangle(linha.x, linha.y, linha.w, linha.h, Color::new(1.0, 1.0, 1.0, if sobre { 0.08 } else { 0.03 }));
            let vermelho = Color::new(0.85, 0.45, 0.40, 1.0);
            let (rotulo, cor) = match &e {
                Estado::Disponivel => ("Disponível".to_string(), Color::new(1.0, 0.84, 0.2, 1.0)),
                Estado::EmAndamento { feito, total } => (format!("Em andamento · {feito}/{total}"), estilo::TEXTO),
                Estado::Pronta => ("Pronta pra entregar".to_string(), estilo::AUTO),
                Estado::Concluida => ("Concluída hoje".to_string(), estilo::SUAVE),
                Estado::Bloqueada(m) => (m.first().cloned().unwrap_or_default(), vermelho),
            };
            let icone = vec2(linha.x + 22.0, linha.y + 26.0);
            match &e {
                Estado::Bloqueada(_) => cadeado(icone, 9.0, vermelho),
                Estado::Concluida => {
                    draw_line(icone.x - 7.0, icone.y, icone.x - 2.0, icone.y + 6.0, 3.0, estilo::SUAVE);
                    draw_line(icone.x - 2.0, icone.y + 6.0, icone.x + 8.0, icone.y - 7.0, 3.0, estilo::SUAVE);
                }
                Estado::Pronta => estilo::texto_centro(icone.x, icone.y + 10.0, "?", 26, estilo::AUTO),
                Estado::Disponivel => estilo::texto_centro(icone.x, icone.y + 10.0, "!", 26, cor),
                Estado::EmAndamento { .. } => draw_circle_lines(icone.x, icone.y, 8.0, 2.0, estilo::TEXTO),
            }
            let apagada = matches!(e, Estado::Bloqueada(_) | Estado::Concluida);
            let largura_texto = linha.w - 170.0;
            estilo::texto_ajustado(d.title, linha.x + 44.0, linha.y + 22.0, largura_texto, 16, if apagada { estilo::SUAVE } else { estilo::TEXTO });
            estilo::texto_ajustado(&rotulo, linha.x + 44.0, linha.y + 42.0, largura_texto, 13, cor);
            estilo::texto_ajustado(&recompensa(d, nomes), linha.x + 44.0, linha.y + 62.0, largura_texto, 13, estilo::OURO);
            if let Some(t) = botao_de(&e) {
                let b = Rect::new(linha.x + linha.w - 122.0, linha.y + 22.0, 110.0, 30.0);
                if crate::ui::botao(b, t, true) {
                    saida = Some(clique_da_diaria(d, &e, c.agora_unix));
                }
            } else if sobre && clicou {
                saida = Some(clique_da_diaria(d, &e, c.agora_unix));
            }
            if let Estado::Bloqueada(m) = &e {
                if sobre {
                    dica = Some(m.clone());
                }
            }
        }
        if let Some(m) = dica {
            let w = m.iter().map(|s| estilo::medir(s, 14)).fold(160.0f32, f32::max) + 24.0;
            let h = 30.0 + m.len() as f32 * 20.0;
            let x = (mouse.x + 16.0).min(screen_width() - w - 8.0);
            let y = (mouse.y + 12.0).min(screen_height() - h - 8.0);
            estilo::painel(Rect::new(x, y, w, h));
            cadeado(vec2(x + 16.0, y + 17.0), 7.0, Color::new(0.85, 0.45, 0.40, 1.0));
            estilo::texto(x + 30.0, y + 22.0, "Pré-requisitos", 14, estilo::OURO);
            for (i, s) in m.iter().enumerate() {
                estilo::texto(x + 12.0, y + 42.0 + i as f32 * 20.0, s, 14, estilo::TEXTO);
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

    fn ctx<'a>(log: &'a [QuestNet], entregues: &'a HashMap<u16, i64>, zona: Option<&'a str>) -> Contexto<'a> {
        Contexto { log, entregues, nivel: 50, faccao: faction_id::PEACEMAIN, zona, agora_unix: 1_000, tem: &nada }
    }

    #[test]
    fn so_as_diarias_da_ilha_em_ordem() {
        let ids: Vec<u16> = da_ilha(Some("ilha_inicial")).iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![601, 602, 603, 604, 605, 606, 607]);
        assert!(da_ilha(None).is_empty(), "fora de ilha nao ha' diaria");
        let gelo = da_ilha(Some("ilha_gelo"));
        assert!(!gelo.is_empty() && gelo.iter().all(|d| d.daily && zona_da_missao(d.id) == Some("ilha_gelo")));
    }

    #[test]
    fn estados_e_botoes_levam_ao_mestre() {
        let vazio = HashMap::new();
        let d601 = quest_by_id(601).unwrap();
        let c = ctx(&[], &vazio, Some("ilha_inicial"));
        let e = estado_da_diaria(d601, &c);
        assert_eq!(e, Estado::Disponivel);
        assert_eq!(botao_de(&e), Some("Ir aceitar"));
        assert_eq!(clique_da_diaria(d601, &e, 1_000), Clique::IrAoGiver(601), "aceitar e' com o Mestre");

        let d606 = quest_by_id(606).unwrap();
        assert_eq!(estado_da_diaria(d606, &c), Estado::Bloqueada(vec!["Em breve".into()]));
        assert_eq!(botao_de(&estado_da_diaria(d606, &c)), None);

        let mut entregues = HashMap::new();
        entregues.insert(601, 50_000);
        let c = ctx(&[], &entregues, Some("ilha_inicial"));
        let e = estado_da_diaria(d601, &c);
        assert_eq!(e, Estado::Concluida, "entregue antes do reset: concluida hoje");
        assert_eq!(botao_de(&e), None);
        assert!(matches!(clique_da_diaria(d601, &e, 1_000), Clique::Aviso(t) if t.contains("reset")));

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
        assert_eq!(botao_de(&e), Some("Ir entregar"));
        assert_eq!(clique_da_diaria(d602, &e, 1_000), Clique::AutoMissao(602), "a auto missao volta ao Mestre");
    }

    #[test]
    fn ponto_vermelho_so_com_algo_pra_aceitar_ou_entregar() {
        let vazio = HashMap::new();
        assert!(tem_pendente(&ctx(&[], &vazio, Some("ilha_inicial"))));
        // Tudo que existe entregue hoje; as "em breve" continuam trancadas.
        let entregues: HashMap<u16, i64> =
            da_ilha(Some("ilha_inicial")).iter().filter(|d| !d.em_breve).map(|d| (d.id, 50_000)).collect();
        assert!(!tem_pendente(&ctx(&[], &entregues, Some("ilha_inicial"))));
        assert!(!tem_pendente(&ctx(&[], &vazio, None)));
    }

    #[test]
    fn recompensa_lista_ouro_xp_e_itens() {
        let d = quest_by_id(601).unwrap();
        let mut nomes = HashMap::new();
        nomes.insert(d.reward_item2, "Poção de Experiência".to_string());
        nomes.insert(d.reward_item, "Item".to_string());
        let r = recompensa(d, &nomes);
        if d.reward_gold > 0 {
            assert!(r.contains("ouro"), "{r}");
        }
        if d.reward_item2 != 0 {
            assert!(r.contains("Poção de Experiência"), "{r}");
        }
    }
}
