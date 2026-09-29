//! What came into the bag, on screen: "+57 Copper" rising from the character.
//!
//! Gathering is automatic and silent (docs/COLETA.md): without this the
//! player only saw the bag grow by opening the bag. It comes from the
//! DIFFERENCE between two `InventoryUpdate` — gathering, loot, purchase and
//! reward all appear without the server sending a new message. It does not go
//! to chat: gathering delivers every second or two and would push out what
//! actually matters to read there.

use std::collections::HashMap;

use macroquad::prelude::*;

use crate::render3d::world_to_screen;

/// How long each line stays on screen.
const VIDA: f32 = 1.8;
/// Lines at once; the oldest leaves.
const MAX_LINHAS: usize = 6;
/// The same item again within this adds to the line already rising.
const JUNTA_S: f32 = 0.35;

#[derive(Default)]
pub struct Ganhos {
    anterior: Option<HashMap<u16, u32>>,
    energia_anterior: Option<u64>,
    /// (source, quantity, age in seconds)
    pub linhas: Vec<(Origem, u64, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    Item(u16),
    Energia,
}

fn totais(slots: &[shared::InventorySlot]) -> HashMap<u16, u32> {
    let mut t = HashMap::new();
    for s in slots.iter().filter(|s| s.qty > 0) {
        *t.entry(s.item_id).or_insert(0) += s.qty;
    }
    t
}

impl Ganhos {
    /// A new bag arrived. Returns what CAME IN (`(item, quantity)`, by id). The
    /// session's first bag does not count: that is the login, not a gain.
    pub fn bolsa_nova(&mut self, slots: &[shared::InventorySlot]) -> Vec<(u16, u32)> {
        let agora = totais(slots);
        let mut entrou: Vec<(u16, u32)> = match &self.anterior {
            None => Vec::new(),
            Some(antes) => agora
                .iter()
                .filter_map(|(id, q)| {
                    let a = antes.get(id).copied().unwrap_or(0);
                    // `then` and not `then_some`: the maths only runs when something came in
                    // — spending would make `q - a` overflow.
                    (*q > a).then(|| (*id, q - a))
                })
                .collect(),
        };
        entrou.sort_unstable();
        self.anterior = Some(agora);
        for &(id, q) in &entrou {
            self.adicionar(Origem::Item(id), q as u64);
        }
        entrou
    }

    /// The new balance came from the server. The first is the login, not a
    /// gather; later increases show as "+N Energy" over the character.
    pub fn energia_nova(&mut self, saldo: u64) -> Option<u64> {
        let ganho = self
            .energia_anterior
            .filter(|antes| saldo > *antes)
            .map(|antes| saldo - antes);
        self.energia_anterior = Some(saldo);
        if let Some(q) = ganho {
            self.adicionar(Origem::Energia, q);
        }
        ganho
    }

    fn adicionar(&mut self, origem: Origem, q: u64) {
        match self
            .linhas
            .iter_mut()
            .find(|(i, _, t)| *i == origem && *t < JUNTA_S)
        {
            Some(l) => l.1 = l.1.saturating_add(q),
            None => self.linhas.push((origem, q, 0.0)),
        }
        let sobra = self.linhas.len().saturating_sub(MAX_LINHAS);
        self.linhas.drain(..sobra);
    }

    pub fn avanca(&mut self, dt: f32) {
        for l in &mut self.linhas {
            l.2 += dt.max(0.0);
        }
        self.linhas.retain(|l| l.2 < VIDA);
    }

    /// Draws in 2D over the world, rising from the head of `pe`.
    pub fn desenha(&mut self, cam: &Camera3D, pe: Vec3, nome: impl Fn(u16) -> String) {
        self.avanca(get_frame_time().min(0.1));
        let Some(c) = world_to_screen(cam, pe + vec3(0.0, 2.3, 0.0)) else {
            return;
        };
        for (k, (origem, q, t)) in self.linhas.iter().rev().enumerate() {
            let u = t / VIDA;
            let alfa = if u < 0.7 { 1.0 } else { 1.0 - (u - 0.7) / 0.3 };
            let y = c.y - 18.0 - k as f32 * 20.0 - 30.0 * u;
            let txt = match origem {
                Origem::Item(id) => format!("+{q} {}", nome(*id)),
                Origem::Energia => format!("+{q} Energia"),
            };
            let tam = 19.0;
            let w = crate::hud_estilo::medir_forte(&txt, tam as u16);
            let x = c.x - w * 0.5;
            let sombra = Color::new(0.0, 0.0, 0.0, 0.85 * alfa);
            for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5)] {
                crate::hud_estilo::texto_forte(x + dx, y + dy, &txt, tam as u16, sombra);
            }
            let cor = if *origem == Origem::Energia {
                Color::new(0.36, 0.87, 1.0, alfa)
            } else {
                Color::new(0.62, 1.0, 0.55, alfa)
            };
            crate::hud_estilo::texto_forte(x, y, &txt, tam as u16, cor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::InventorySlot;

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn so_o_que_entra_depois_do_login_aparece() {
        let mut g = Ganhos::default();
        assert!(
            g.bolsa_nova(&[slot(344, 500), slot(300, 3)]).is_empty(),
            "login nao e' ganho"
        );
        let entrou = g.bolsa_nova(&[slot(344, 557), slot(300, 3), slot(301, 4)]);
        assert_eq!(entrou, vec![(300 + 1, 4), (344, 57)]);
        // Spending/selling does not show as a gain.
        assert!(g.bolsa_nova(&[slot(344, 10)]).is_empty());
        assert_eq!(g.linhas.len(), 2);
    }

    #[test]
    fn mesmo_item_seguido_soma_na_linha_e_linha_velha_some() {
        let mut g = Ganhos::default();
        g.bolsa_nova(&[]);
        g.bolsa_nova(&[slot(344, 40)]);
        g.bolsa_nova(&[slot(344, 100)]);
        assert_eq!(g.linhas, vec![(Origem::Item(344), 100, 0.0)]);
        g.avanca(VIDA + 0.1);
        assert!(g.linhas.is_empty());
        // A stack in two slots counts together.
        g.bolsa_nova(&[slot(344, 60), slot(344, 60)]);
        assert_eq!(g.linhas, vec![(Origem::Item(344), 20, 0.0)]);
    }

    #[test]
    fn energia_do_login_nao_pisca_mas_coleta_aparece_e_soma() {
        let mut g = Ganhos::default();
        assert_eq!(g.energia_nova(1_200), None);
        assert!(g.linhas.is_empty());
        assert_eq!(g.energia_nova(1_212), Some(12));
        assert_eq!(g.energia_nova(1_224), Some(12));
        assert_eq!(g.linhas, vec![(Origem::Energia, 24, 0.0)]);
        assert_eq!(g.energia_nova(1_200), None, "gasto nao aparece como ganho");
    }
}
