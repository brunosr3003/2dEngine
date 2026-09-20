//! A faixa "o que eu tenho" do pet e da montaria.
//!
//! As duas telas mostram a mesma coisa — tudo daquela família que está na
//! bolsa, mais a peça equipada —, e de lá se **equipa** e se **combina**. Por
//! isso mora num lugar só: mudar a regra num painel mudava no outro e o outro
//! ficava para trás.
//!
//! Equipar é `UseItem` no slot, o mesmo caminho da bolsa. Combinar é
//! `Combinar`, o mesmo da aba Combinar do Craft — nada aqui é caminho novo de
//! servidor.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::InventorySlot;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

#[derive(Default)]
pub struct Colecao {
    /// Slot da bolsa escolhido. `None` = nenhum.
    pub sel: Option<usize>,
}

/// Uma peça da coleção: onde está na bolsa e o que é.
pub struct Peca {
    pub slot: usize,
    pub item_id: u16,
    pub qty: u32,
}

impl Colecao {
    /// Desenha a faixa e devolve o pedido, se houver.
    ///
    /// `da_familia` diz se um item_id pertence à família (pet ou montaria);
    /// `nome` monta o rótulo da peça escolhida.
    #[allow(clippy::too_many_arguments)]
    pub fn desenha(
        &mut self,
        r: Rect,
        f: f32,
        titulo: &str,
        bolsa: &[InventorySlot],
        equipado: Option<u16>,
        da_familia: &dyn Fn(u16) -> bool,
        nome: &dyn Fn(u16) -> String,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        let mouse = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        estilo::cartao(r, false, false);
        estilo::texto_forte(r.x + 14.0 * f, r.y + 24.0 * f, titulo, 15, estilo::OURO);

        let pecas: Vec<Peca> = bolsa
            .iter()
            .enumerate()
            .filter(|(_, s)| s.qty > 0 && da_familia(s.item_id))
            .map(|(slot, s)| Peca {
                slot,
                item_id: s.item_id,
                qty: s.qty,
            })
            .collect();

        if pecas.is_empty() {
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 56.0 * f,
                "Nada na bolsa. O pergaminho da Loja traz um.",
                14,
                estilo::SUAVE,
            );
            self.sel = None;
            return None;
        }
        // A escolha some quando o item sai da bolsa (equipou, combinou).
        if !self.sel.is_some_and(|i| pecas.iter().any(|p| p.slot == i)) {
            self.sel = pecas.first().map(|p| p.slot);
        }

        // ── as celulas, numa fila ──
        let lado = 56.0 * f;
        let vao = 8.0 * f;
        let y = r.y + 34.0 * f;
        let cabem = (((r.w - 28.0 * f) / (lado + vao)).floor() as usize).max(1);
        for (i, p) in pecas.iter().take(cabem).enumerate() {
            let c = Rect::new(r.x + 14.0 * f + i as f32 * (lado + vao), y, lado, lado);
            crate::bolsa::celula_avulsa(c, p.item_id, p.qty, self.sel == Some(p.slot), palco);
            if clique && c.contains(mouse) {
                self.sel = Some(p.slot);
            }
        }
        if pecas.len() > cabem {
            estilo::texto(
                r.x + 14.0 * f + cabem as f32 * (lado + vao),
                y + lado * 0.6,
                &format!("+{}", pecas.len() - cabem),
                14,
                estilo::SUAVE,
            );
        }

        // ── o que dá pra fazer com a escolhida ──
        let Some(escolhida) = self.sel.and_then(|i| pecas.iter().find(|p| p.slot == i)) else {
            return None;
        };
        let by = y + lado + 12.0 * f;
        estilo::texto_ajustado(
            &nome(escolhida.item_id),
            r.x + 14.0 * f,
            by + 16.0 * f,
            r.w - 300.0 * f,
            15,
            estilo::TEXTO,
        );

        let bw = 132.0 * f;
        let bt_eq = Rect::new(r.x + r.w - bw * 2.0 - 22.0 * f, by, bw, 32.0 * f);
        let ja = equipado == Some(escolhida.item_id);
        estilo::botao(
            bt_eq,
            if ja { "Equipada" } else { "Equipar" },
            estilo::estado_de(bt_eq, ja, false),
            !ja,
        );
        if clique && !ja && bt_eq.contains(mouse) {
            return Some(ClientMessage::UseItem {
                slot: escolhida.slot as u16,
            });
        }

        // Combinar: 3 iguais tentam 1 do grau de cima. O botão só acende com
        // as três na mão — a recusa do servidor não cobraria nada, mas prometer
        // o que não dá é pior que não oferecer.
        let receita = shared::combinar::receita(escolhida.item_id);
        let bt_co = Rect::new(r.x + r.w - bw - 14.0 * f, by, bw, 32.0 * f);
        match receita {
            Some(rc) => {
                let tem: u32 = bolsa
                    .iter()
                    .filter(|s| s.item_id == escolhida.item_id)
                    .map(|s| s.qty)
                    .sum();
                let pode = tem >= rc.qtd;
                estilo::botao(
                    bt_co,
                    &format!("Combinar {}/{}", tem.min(rc.qtd), rc.qtd),
                    estilo::estado_de(bt_co, !pode, false),
                    pode,
                );
                estilo::texto(
                    r.x + 14.0 * f,
                    by + 36.0 * f,
                    &format!(
                        "Combinar: {} iguais tentam o grau de cima, {}% de chance, {} de cobre. Falhar consome as {}.",
                        rc.qtd,
                        rc.chance,
                        crate::bolsa::milhar(rc.cobre as u64),
                        rc.qtd
                    ),
                    12,
                    estilo::SUAVE,
                );
                if clique && pode && bt_co.contains(mouse) {
                    return Some(ClientMessage::Combinar {
                        entrada: escolhida.item_id,
                        vezes: 1,
                    });
                }
            }
            None => {
                estilo::botao(bt_co, "No topo", estilo::estado_de(bt_co, true, false), false);
                estilo::texto(
                    r.x + 14.0 * f,
                    by + 36.0 * f,
                    "Laranja é o topo: não há grau acima para combinar.",
                    12,
                    estilo::SUAVE,
                );
            }
        }
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn slot(item_id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id,
            qty,
            instance: None,
        }
    }

    /// A faixa lista o que esta' na bolsa daquela familia — e so' isso.
    #[test]
    fn a_familia_filtra_o_que_aparece() {
        let pet = shared::item_id::pet_no_grau(shared::item_id::PET_LOBO, 2);
        let montaria = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_URSO, 1);
        let bolsa = vec![
            slot(shared::item_id::HEALTH_POTION, 5),
            slot(pet, 1),
            slot(montaria, 1),
            slot(0, 0),
        ];
        let so_pet = |id: u16| shared::pets::de_item(id).is_some();
        let so_montaria = |id: u16| shared::montarias::de_item(id).is_some();
        let conta = |f: &dyn Fn(u16) -> bool| {
            bolsa
                .iter()
                .filter(|s| s.qty > 0 && f(s.item_id))
                .count()
        };
        assert_eq!(conta(&so_pet), 1);
        assert_eq!(conta(&so_montaria), 1);
        // Slot vazio nunca entra, nem que o id caia na faixa por acaso.
        assert!(bolsa.iter().any(|s| s.qty == 0));
    }

    /// O topo nao tem receita: o botao de combinar tem que dizer isso em vez
    /// de prometer um grau que nao existe.
    #[test]
    fn o_laranja_nao_tem_para_onde_subir() {
        for base in [shared::item_id::PET_LOBO, shared::item_id::MONTARIA_URSO] {
            let topo = if base == shared::item_id::PET_LOBO {
                shared::item_id::pet_no_grau(base, shared::pets::GRAU_MAX)
            } else {
                shared::item_id::montaria_no_grau(base, shared::montarias::GRAU_MAX)
            };
            assert!(shared::combinar::receita(topo).is_none(), "{topo}");
            let abaixo = topo - 1;
            let r = shared::combinar::receita(abaixo).expect("o roxo sobe");
            assert_eq!(r.saida, topo);
            assert_eq!(r.qtd, 3);
        }
    }
}
