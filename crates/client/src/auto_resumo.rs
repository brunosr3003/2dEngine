//! Summary of the auto-quest session. Only accumulates events the server confirmed.
use std::collections::{BTreeMap, HashMap};

use macroquad::prelude::*;

use crate::hud_estilo as estilo;

#[derive(Default)]
pub struct AutoResumo {
    pub ativo: bool,
    concluidas: Vec<Concluida>,
    coletados: BTreeMap<u16, u64>,
    energia: u64,
    xp: u64,
    cobre: u64,
    itens_de_missao: BTreeMap<u16, u64>,
}

struct Concluida {
    id: u16,
    nome: String,
    premio: String,
}

impl AutoResumo {
    pub fn iniciar(&mut self) { *self = Self { ativo: true, ..Self::default() }; }
    pub fn parar(&mut self) { self.ativo = false; }

    pub fn concluir(&mut self, d: &shared::quests::QuestDef, nomes: &HashMap<u16, String>) {
        if !self.ativo || self.concluidas.iter().any(|c| c.id == d.id) { return; }
        let mut partes = Vec::new();
        if d.reward_xp > 0 { partes.push(format!("{} XP", d.reward_xp)); }
        if d.reward_cobre > 0 { partes.push(format!("{} cobre", d.reward_cobre)); }
        for (id, qtd) in [(d.reward_item, d.reward_item_qty), (d.reward_item2, d.reward_item2_qty)] {
            if id != 0 && qtd > 0 {
                partes.push(format!("{} {}", qtd,
                    nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"))));
            }
        }
        self.concluidas.push(Concluida { id: d.id, nome: d.title.into(),
            premio: if partes.is_empty() { "Sem recompensa".into() } else { partes.join(" · ") } });
        self.xp = self.xp.saturating_add(d.reward_xp);
        self.cobre = self.cobre.saturating_add(d.reward_cobre as u64);
        for (id, qtd) in [(d.reward_item, d.reward_item_qty), (d.reward_item2, d.reward_item2_qty)] {
            if id != 0 && qtd > 0 { *self.itens_de_missao.entry(id).or_default() += qtd as u64; }
        }
    }

    pub fn coletar_itens(&mut self, itens: &[(u16, u32)]) {
        if !self.ativo { return; }
        for &(id, qtd) in itens {
            *self.coletados.entry(id).or_default() += qtd as u64;
        }
    }

    pub fn coletar_energia(&mut self, qtd: u64) {
        if self.ativo { self.energia = self.energia.saturating_add(qtd); }
    }

    pub fn desenhar(&self, r: Rect, atual: Option<(u16, &str, u32, u32)>,
        proximas: &[u16], nomes: &HashMap<u16, String>) {
        if !self.ativo { return; }
        let s = estilo::fator_texto();
        estilo::painel(r);
        let x = r.x + 12.0 * s;
        let w = r.w - 24.0 * s;
        let compacto = r.h < 150.0 * s;
        estilo::texto_ajustado(&format!("AUTO MISSÃO  ·  {} concluída(s)", self.concluidas.len()),
            x, r.y + 21.0 * s, w, 14, estilo::OURO);
        let agora = atual.map(|(_, nome, feito, total)|
            format!("Fazendo: {nome}  {feito}/{total}"))
            .unwrap_or_else(|| "Escolhendo próxima missão...".into());
        estilo::texto_ajustado(&agora, x, r.y + 43.0 * s, w, 13, estilo::TEXTO);
        if compacto {
            if let Some(c) = self.concluidas.last() {
                estilo::texto_ajustado(&format!("Última: {} · {}", c.nome, c.premio),
                    x, r.y + 64.0 * s, w, 12, estilo::VERDE);
            }
            let coletado = self.texto_coletado(nomes);
            estilo::texto_ajustado(&format!("Coletado: {coletado}"),
                x, r.y + 85.0 * s, w, 12, estilo::VERDE);
            return;
        }
        if !proximas.is_empty() {
            let outros: Vec<&str> = proximas.iter().take(2)
                .filter_map(|id| shared::quests::quest_by_id(*id).map(|d| d.title)).collect();
            let resto = proximas.len().saturating_sub(outros.len());
            let texto = format!("Na fila: {}{}", outros.join(", "),
                if resto > 0 { format!(" +{resto}") } else { String::new() });
            estilo::texto_ajustado(&texto, x, r.y + 62.0 * s, w, 12, estilo::SUAVE);
        }
        if let Some(c) = self.concluidas.last() {
            estilo::texto_ajustado(&format!("Concluída: {} · {}", c.nome, c.premio),
                x, r.y + 84.0 * s, w, 12, estilo::VERDE);
        } else {
            estilo::texto(x, r.y + 84.0 * s, "Recompensas: aguardando conclusão", 12, estilo::SUAVE);
        }
        estilo::texto_ajustado(&format!("Coletado: {}", self.texto_coletado(nomes)),
            x, r.y + 107.0 * s, w, 12, estilo::VERDE);
        if self.concluidas.len() > 1 {
            let anterior = &self.concluidas[self.concluidas.len() - 2];
            estilo::texto_ajustado(&format!("Anterior: {} · {}", anterior.nome, anterior.premio),
                x, r.y + 130.0 * s, w, 11, estilo::SUAVE);
        }
        let mut premios = Vec::new();
        if self.xp > 0 { premios.push(format!("{} XP", self.xp)); }
        if self.cobre > 0 { premios.push(format!("{} cobre", self.cobre)); }
        for (&id, &qtd) in &self.itens_de_missao {
            premios.push(format!("{} {}", qtd,
                nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"))));
        }
        estilo::texto_ajustado(&format!("Total recebido: {}", if premios.is_empty() {
            "—".into()
        } else { premios.join(" · ") }), x, r.y + 153.0 * s, w, 11, estilo::OURO);
    }

    fn texto_coletado(&self, nomes: &HashMap<u16, String>) -> String {
        let mut ganhos = Vec::new();
        if self.energia > 0 { ganhos.push(format!("{} Energia", self.energia)); }
        for (&id, &qtd) in &self.coletados {
            ganhos.push(format!("{} {}", qtd,
                nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"))));
        }
        if ganhos.is_empty() { "—".into() } else { ganhos.join(" · ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumo_soma_coleta_e_ignora_eventos_fora_da_sessao() {
        let mut r = AutoResumo::default();
        r.coletar_itens(&[(345, 4)]);
        r.coletar_energia(10);
        assert!(r.coletados.is_empty());
        r.iniciar();
        r.coletar_itens(&[(345, 4), (345, 3)]);
        r.coletar_energia(10);
        assert_eq!(r.coletados[&345], 7);
        assert_eq!(r.energia, 10);
        r.parar();
        r.coletar_itens(&[(345, 20)]);
        assert_eq!(r.coletados[&345], 7);
    }

    #[test]
    fn conclusao_conta_uma_vez_e_acumula_premios() {
        let mut r = AutoResumo::default();
        r.iniciar();
        let d = shared::quests::quest_by_id(501).unwrap();
        r.concluir(d, &HashMap::new());
        r.concluir(d, &HashMap::new());
        assert_eq!(r.concluidas.len(), 1);
        assert_eq!(r.xp, d.reward_xp);
        assert_eq!(r.cobre, d.reward_cobre as u64);
    }
}
