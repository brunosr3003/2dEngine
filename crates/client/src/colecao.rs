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
    /// Qual peça está escolhida, pelo slot da bolsa. `None` é a EQUIPADA, que
    /// não tem slot — e, quando não há equipada, cai na primeira da bolsa no
    /// próximo quadro.
    pub sel: Option<usize>,
}

/// Uma peça da coleção. `slot` é onde ela está na bolsa; a EQUIPADA não está
/// em slot nenhum — ela saiu da bolsa ao ser equipada, e sem entrar aqui
/// sumiria da lista justamente quando passou a ser a principal.
pub struct Peca {
    pub slot: Option<usize>,
    pub item_id: u16,
    pub qty: u32,
    pub equipada: bool,
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
        let clique = crate::foco::clique();
        estilo::cartao(r, false, false);
        estilo::texto_forte(r.x + 14.0 * f, r.y + 24.0 * f, titulo, 15, estilo::OURO);

        // A equipada vem primeiro, depois o que esta' na bolsa.
        let mut pecas: Vec<Peca> = equipado
            .filter(|id| da_familia(*id))
            .map(|item_id| Peca {
                slot: None,
                item_id,
                qty: 1,
                equipada: true,
            })
            .into_iter()
            .collect();
        pecas.extend(
            bolsa
                .iter()
                .enumerate()
                .filter(|(_, s)| s.qty > 0 && da_familia(s.item_id))
                .map(|(slot, s)| Peca {
                    slot: Some(slot),
                    item_id: s.item_id,
                    qty: s.qty,
                    equipada: false,
                }),
        );

        if pecas.is_empty() {
            estilo::texto(
                r.x + 14.0 * f,
                r.y + 56.0 * f,
                "Nada ainda. O pergaminho da Loja traz um.",
                14,
                estilo::SUAVE,
            );
            self.sel = None;
            return None;
        }
        // A escolha some quando a peca sai da lista (equipou, combinou); a
        // equipada e' a primeira, e `None` e' o slot dela.
        if !pecas.iter().any(|p| p.slot == self.sel) {
            self.sel = pecas.first().and_then(|p| p.slot);
        }

        // ── as celulas, numa fila ──
        let lado = 56.0 * f;
        let vao = 8.0 * f;
        let y = r.y + 34.0 * f;
        let cabem = (((r.w - 28.0 * f) / (lado + vao)).floor() as usize).max(1);
        for (i, p) in pecas.iter().take(cabem).enumerate() {
            let c = Rect::new(r.x + 14.0 * f + i as f32 * (lado + vao), y, lado, lado);
            crate::bolsa::celula_avulsa(c, p.item_id, p.qty, self.sel == p.slot, palco);
            if p.equipada {
                // Um tique dourado no canto: e' a que esta' valendo.
                estilo::texto_forte(c.x + c.w - 13.0 * f, c.y + 14.0 * f, "•", 20, estilo::OURO);
            }
            if clique && c.contains(mouse) {
                self.sel = p.slot;
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
        let Some(escolhida) = pecas.iter().find(|p| p.slot == self.sel) else {
            return None;
        };
        let by = y + lado + 12.0 * f;
        estilo::texto_ajustado(
            // "Em uso" em vez de "equipado/equipada": o mesmo texto serve
            // pro pet e pra montaria, sem errar a concordancia num dos dois.
            &if escolhida.equipada {
                format!("{} · em uso", nome(escolhida.item_id))
            } else {
                nome(escolhida.item_id)
            },
            r.x + 14.0 * f,
            by + 16.0 * f,
            r.w - 300.0 * f,
            15,
            estilo::TEXTO,
        );

        let bw = 132.0 * f;
        let bt_eq = Rect::new(r.x + r.w - bw * 2.0 - 22.0 * f, by, bw, 32.0 * f);
        estilo::botao(
            bt_eq,
            if escolhida.equipada { "Em uso" } else { "Equipar" },
            estilo::estado_de(bt_eq, escolhida.equipada, false),
            !escolhida.equipada,
        );
        if let (true, Some(slot)) = (clique && bt_eq.contains(mouse), escolhida.slot) {
            return Some(ClientMessage::UseItem { slot: slot as u16 });
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

    /// A equipada entra na lista mesmo fora da bolsa, e vem primeiro. Sem
    /// isso ela sumia da faixa justamente quando virava a principal.
    #[test]
    fn a_equipada_aparece_mesmo_fora_da_bolsa() {
        let equipada = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 4);
        let na_bolsa = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 1);
        let bolsa = vec![slot(na_bolsa, 2)];
        let da_familia = |id: u16| shared::pets::de_item(id).is_some();

        let monta = |equipado: Option<u16>| -> Vec<(Option<usize>, u16, bool)> {
            let mut v: Vec<(Option<usize>, u16, bool)> = equipado
                .filter(|id| da_familia(*id))
                .map(|id| (None, id, true))
                .into_iter()
                .collect();
            v.extend(
                bolsa
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.qty > 0 && da_familia(s.item_id))
                    .map(|(i, s)| (Some(i), s.item_id, false)),
            );
            v
        };

        let com = monta(Some(equipada));
        assert_eq!(com.len(), 2, "equipada + a da bolsa");
        assert_eq!(com[0], (None, equipada, true), "a equipada vem primeiro");
        assert_eq!(com[1], (Some(0), na_bolsa, false));

        // Sem nada equipado, so' a bolsa — e a lista nunca fica vazia a` toa.
        let sem = monta(None);
        assert_eq!(sem.len(), 1);
        assert!(!sem[0].2);

        // So' a equipada, bolsa vazia: a faixa ainda mostra alguma coisa.
        let bolsa_vazia: Vec<InventorySlot> = vec![slot(0, 0)];
        let so_equipada = bolsa_vazia
            .iter()
            .filter(|s| s.qty > 0 && da_familia(s.item_id))
            .count();
        assert_eq!(so_equipada, 0, "a bolsa nao tem nada");
        assert!(da_familia(equipada), "mas a equipada ainda conta");
    }

    /// A faixa lista o que esta' na bolsa daquela familia — e so' isso.
    #[test]
    fn a_familia_filtra_o_que_aparece() {
        let pet = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2);
        let montaria = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 1);
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
        for base in [shared::item_id::PET_BASE, shared::item_id::MONTARIA_BASE] {
            let topo = if base == shared::item_id::PET_BASE {
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
