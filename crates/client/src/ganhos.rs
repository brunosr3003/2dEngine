//! O que entrou na bolsa, na tela: "+57 Cobre" subindo do personagem.
//!
//! A coleta e' automatica e silenciosa (docs/COLETA.md): sem isto o jogador
//! so' via a bolsa crescer se abrisse a bolsa. Sai da DIFERENCA entre duas
//! `InventoryUpdate` — coleta, loot, compra e recompensa aparecem sem o
//! servidor mandar mensagem nova. Nao vai pro chat: a coleta entrega a cada
//! um ou dois segundos e empurraria pra fora o que importa ler la'.

use std::collections::HashMap;

use macroquad::prelude::*;

use crate::render3d::world_to_screen;

/// Quanto cada linha fica na tela.
const VIDA: f32 = 1.8;
/// Linhas ao mesmo tempo; a mais velha sai.
const MAX_LINHAS: usize = 6;
/// O mesmo item de novo dentro disto soma na linha que ja' esta' subindo.
const JUNTA_S: f32 = 0.35;

#[derive(Default)]
pub struct Ganhos {
    anterior: Option<HashMap<u16, u32>>,
    /// (item, quantidade, idade em segundos)
    pub linhas: Vec<(u16, u32, f32)>,
}

fn totais(slots: &[shared::InventorySlot]) -> HashMap<u16, u32> {
    let mut t = HashMap::new();
    for s in slots.iter().filter(|s| s.qty > 0) {
        *t.entry(s.item_id).or_insert(0) += s.qty;
    }
    t
}

impl Ganhos {
    /// Chegou bolsa nova. Devolve o que ENTROU (`(item, quantidade)`, por id).
    /// A primeira bolsa da sessao nao conta: e' o login, nao um ganho.
    pub fn bolsa_nova(&mut self, slots: &[shared::InventorySlot]) -> Vec<(u16, u32)> {
        let agora = totais(slots);
        let mut entrou: Vec<(u16, u32)> = match &self.anterior {
            None => Vec::new(),
            Some(antes) => agora
                .iter()
                .filter_map(|(id, q)| {
                    let a = antes.get(id).copied().unwrap_or(0);
                    // `then` e nao `then_some`: a conta so' roda quando entrou
                    // — gastar faria `q - a` estourar.
                    (*q > a).then(|| (*id, q - a))
                })
                .collect(),
        };
        entrou.sort_unstable();
        self.anterior = Some(agora);
        for &(id, q) in &entrou {
            match self.linhas.iter_mut().find(|(i, _, t)| *i == id && *t < JUNTA_S) {
                Some(l) => l.1 += q,
                None => self.linhas.push((id, q, 0.0)),
            }
        }
        let sobra = self.linhas.len().saturating_sub(MAX_LINHAS);
        self.linhas.drain(..sobra);
        entrou
    }

    pub fn avanca(&mut self, dt: f32) {
        for l in &mut self.linhas {
            l.2 += dt.max(0.0);
        }
        self.linhas.retain(|l| l.2 < VIDA);
    }

    /// Desenha em 2D por cima do mundo, subindo da cabeca de `pe`.
    pub fn desenha(&mut self, cam: &Camera3D, pe: Vec3, nome: impl Fn(u16) -> String) {
        self.avanca(get_frame_time().min(0.1));
        let Some(c) = world_to_screen(cam, pe + vec3(0.0, 2.3, 0.0)) else { return };
        for (k, (id, q, t)) in self.linhas.iter().rev().enumerate() {
            let u = t / VIDA;
            let alfa = if u < 0.7 { 1.0 } else { 1.0 - (u - 0.7) / 0.3 };
            let y = c.y - 18.0 - k as f32 * 20.0 - 30.0 * u;
            let txt = format!("+{q} {}", nome(*id));
            let tam = 19.0;
            let w = crate::hud_estilo::medir_forte(&txt, tam as u16);
            let x = c.x - w * 0.5;
            let sombra = Color::new(0.0, 0.0, 0.0, 0.85 * alfa);
            for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5)] {
                crate::hud_estilo::texto_forte(x + dx, y + dy, &txt, tam as u16, sombra);
            }
            crate::hud_estilo::texto_forte(x, y, &txt, tam as u16, Color::new(0.62, 1.0, 0.55, alfa));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::InventorySlot;

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot { item_id: id, qty, instance: None }
    }

    #[test]
    fn so_o_que_entra_depois_do_login_aparece() {
        let mut g = Ganhos::default();
        assert!(g.bolsa_nova(&[slot(344, 500), slot(300, 3)]).is_empty(), "login nao e' ganho");
        let entrou = g.bolsa_nova(&[slot(344, 557), slot(300, 3), slot(301, 4)]);
        assert_eq!(entrou, vec![(300 + 1, 4), (344, 57)]);
        // Gastar/vender nao aparece como ganho.
        assert!(g.bolsa_nova(&[slot(344, 10)]).is_empty());
        assert_eq!(g.linhas.len(), 2);
    }

    #[test]
    fn mesmo_item_seguido_soma_na_linha_e_linha_velha_some() {
        let mut g = Ganhos::default();
        g.bolsa_nova(&[]);
        g.bolsa_nova(&[slot(344, 40)]);
        g.bolsa_nova(&[slot(344, 100)]);
        assert_eq!(g.linhas, vec![(344, 100, 0.0)]);
        g.avanca(VIDA + 0.1);
        assert!(g.linhas.is_empty());
        // Pilha em dois slots conta junta.
        g.bolsa_nova(&[slot(344, 60), slot(344, 60)]);
        assert_eq!(g.linhas, vec![(344, 20, 0.0)]);
    }
}
