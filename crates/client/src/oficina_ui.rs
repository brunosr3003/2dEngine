//! As duas abas de oficina do painel de Craft: APRIMORAR (duas pecas iguais
//! viram uma do tier seguinte) e COMBINAR (chave/material de uma cor tenta a
//! cor de cima).
//!
//! Mesma regra do resto do painel: nada aqui decide. O servidor confere e
//! sorteia (`Aprimorar` → `AprimorarResultado`, `Combinar` →
//! `CombinarResultado`); o painel so' le' a bolsa pra mostrar o que da' e nao
//! mandar pedido que volta recusado.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::combinar::{self, ReceitaDeCombinar};
use shared::forja;
use shared::protocol::ClientMessage;
use shared::{item_id, InventorySlot};

use crate::craft_ui::tem;
use crate::hud_estilo as estilo;

const LINHA: f32 = 44.0;
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);
const VERMELHO: Color = Color::new(0.90, 0.40, 0.34, 1.0);
const AMARELO: Color = Color::new(0.95, 0.78, 0.30, 1.0);
const ROMANO: [&str; 5] = ["I", "II", "III", "IV", "V"];

pub fn romano(tier: u8) -> &'static str {
    ROMANO[(tier.clamp(1, 5) - 1) as usize]
}

// ─────────────────────────────── aprimorar ───────────────────────────────

/// Pecas iguais da bolsa: mesmo item, mesmo tier. `slots` em ordem de refino
/// crescente — as duas primeiras sao as que vao pra fusao (perde-se menos).
#[derive(Debug, Clone, PartialEq)]
pub struct Grupo {
    pub item_id: u16,
    pub tier: u8,
    pub slots: Vec<usize>,
}

impl Grupo {
    /// Da' pra fundir (sem contar o cobre)?
    pub fn fundivel(&self) -> bool {
        self.slots.len() >= 2 && self.tier < forja::APRIMORAR_TIER_MAX
    }
}

/// As pecas de equipamento da bolsa agrupadas pra aba: as que se fundem
/// primeiro, depois as que ainda esperam a segunda igual. Peca com gema fica
/// de fora (o servidor recusa).
pub fn grupos(slots: &[InventorySlot]) -> Vec<Grupo> {
    let mut v: Vec<Grupo> = Vec::new();
    for (i, s) in slots.iter().enumerate() {
        let Some(inst) = s.instance.filter(|_| s.qty > 0) else {
            continue;
        };
        if inst.socketed_gems.iter().any(|g| *g != 0) {
            continue;
        }
        let tier = inst.tier();
        match v.iter_mut().find(|g| g.item_id == s.item_id && g.tier == tier) {
            Some(g) => g.slots.push(i),
            None => v.push(Grupo {
                item_id: s.item_id,
                tier,
                slots: vec![i],
            }),
        }
    }
    for g in &mut v {
        g.slots.sort_by_key(|&i| (slots[i].instance.map_or(0, |x| x.refinement), i));
    }
    v.sort_by_key(|g| (!g.fundivel(), g.item_id, g.tier));
    v
}

/// O maior refino que a fusao do grupo joga fora (das duas escolhidas).
pub fn refino_perdido(g: &Grupo, slots: &[InventorySlot]) -> u8 {
    g.slots
        .iter()
        .take(2)
        .filter_map(|&i| slots[i].instance.map(|x| x.refinement))
        .max()
        .unwrap_or(0)
}

/// Por que o grupo nao se funde agora (`None` = funde).
pub fn motivo_aprimorar(g: &Grupo, slots: &[InventorySlot]) -> Option<String> {
    if g.tier >= forja::APRIMORAR_TIER_MAX {
        return Some("Tier IV é o máximo do Aprimorar".into());
    }
    if g.slots.len() < 2 {
        return Some("Precisa de mais uma peça igual (mesmo item e tier)".into());
    }
    let cobre = forja::custo_de_aprimorar(g.tier);
    let t = tem(slots, item_id::COPPER);
    (t < cobre).then(|| format!("Faltam {} de cobre", cobre - t))
}

// ─────────────────────────────── combinar ────────────────────────────────

/// As receitas na ordem da aba: primeiro as que da' pra tentar, depois as de
/// que se tem alguma coisa, depois o resto (pra mostrar o caminho). Dentro de
/// cada faixa, chaves antes de material e cor crescente.
pub fn receitas(slots: &[InventorySlot]) -> Vec<ReceitaDeCombinar> {
    let mut v = combinar::receitas();
    let t = |id: u16| tem(slots, id);
    v.sort_by_key(|r| {
        let faixa = if combinar::vezes_possiveis(r, &t) > 0 {
            0
        } else if t(r.entrada) > 0 {
            1
        } else {
            2
        };
        (faixa, !r.chave, r.cor, r.entrada)
    });
    v
}

/// A frase de um `CombinarResultado`.
pub fn texto_do_resultado(
    tentativas: u16,
    sucessos: u16,
    saida: &str,
    motivo: &str,
) -> (String, bool) {
    if tentativas == 0 {
        return (format!("Não combinou: {motivo}"), false);
    }
    if sucessos == 0 {
        return (
            format!("{tentativas} tentativa(s), nenhuma deu certo."),
            false,
        );
    }
    (
        format!("{tentativas} tentativa(s): {sucessos}x {saida}!"),
        true,
    )
}

// ────────────────────────────── desenho ─────────────────────────────────

#[derive(Default)]
pub struct Oficina {
    /// Grupo escolhido no Aprimorar: (item, tier).
    sel_grupo: Option<(u16, u8)>,
    /// Receita escolhida no Combinar (a entrada).
    sel_receita: Option<u16>,
    rolagem: f32,
    /// Lupa tocada: o item pro "Onde obter".
    pub onde_obter: Option<u16>,
}

/// Seta "vira" entre dois icones. Desenhada: a fonte do HUD nao tem o "→".
fn seta(x: f32, y: f32) {
    let c = estilo::OURO;
    draw_rectangle(x, y - 3.0, 18.0, 6.0, c);
    draw_triangle(
        Vec2::new(x + 16.0, y - 10.0),
        Vec2::new(x + 16.0, y + 10.0),
        Vec2::new(x + 28.0, y),
        c,
    );
}

fn fundo_da_lista(lista: Rect) {
    draw_rectangle(
        lista.x,
        lista.y,
        lista.w,
        lista.h,
        Color::new(0.0, 0.0, 0.0, 0.25),
    );
}

fn realce(linha: Rect, marcada: bool, sobre: bool) {
    let a = if marcada {
        0.14
    } else if sobre {
        0.07
    } else {
        0.0
    };
    draw_rectangle(
        linha.x,
        linha.y,
        linha.w,
        linha.h,
        Color::new(1.0, 1.0, 1.0, a),
    );
}

/// Uma linha "icone  nome ........ tem/precisa" com a lupa do Onde obter.
fn linha_de_custo(
    d: Rect,
    y: f32,
    id: u16,
    nome: &str,
    t: u32,
    precisa: u32,
    onde: &mut Option<u16>,
) {
    crate::bolsa::icone_do_item(Rect::new(d.x + 6.0, y, 30.0, 30.0), id, 1.0);
    estilo::texto_ajustado(nome, d.x + 44.0, y + 20.0, d.w - 210.0, 15, estilo::TEXTO);
    let txt = format!("{}/{}", crate::bolsa::milhar(t as u64), crate::bolsa::milhar(precisa as u64));
    let cor = if t >= precisa { VERDE } else { VERMELHO };
    estilo::texto(
        d.x + d.w - estilo::medir(&txt, 15) - 50.0,
        y + 20.0,
        &txt,
        15,
        cor,
    );
    if crate::onde_obter::botao(Rect::new(d.x + d.w - 40.0, y - 1.0, 34.0, 32.0)) {
        *onde = Some(id);
    }
}

impl Oficina {
    fn rolar(&mut self, lista: Rect, total: f32) {
        if lista.contains(Vec2::from(mouse_position())) {
            let (_, roda) = mouse_wheel();
            self.rolagem =
                (self.rolagem - roda.signum() * LINHA).clamp(0.0, (total - lista.h).max(0.0));
        }
    }

    pub fn trocou_de_aba(&mut self) {
        self.rolagem = 0.0;
    }

    /// Aba Aprimorar. `lista` e `d` sao as duas colunas do painel.
    pub fn aprimorar(
        &mut self,
        lista: Rect,
        d: Rect,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
    ) -> Option<ClientMessage> {
        let nome = |id: u16| nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"));
        fundo_da_lista(lista);
        let gs = grupos(slots);
        self.rolar(lista, gs.len() as f32 * LINHA);
        if gs.is_empty() {
            estilo::texto(lista.x + 10.0, lista.y + 24.0, "Nenhuma peça na bolsa.", 14, estilo::SUAVE);
            estilo::texto(lista.x + 10.0, lista.y + 44.0, "Crie no Craft ou guarde as que caírem.", 13, estilo::SUAVE);
        }
        if self.sel_grupo.is_none_or(|s| !gs.iter().any(|g| (g.item_id, g.tier) == s)) {
            self.sel_grupo = gs.first().map(|g| (g.item_id, g.tier));
        }
        let mouse = Vec2::from(mouse_position());
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        for (i, g) in gs.iter().enumerate() {
            let y = lista.y + i as f32 * LINHA - self.rolagem;
            if y + LINHA < lista.y || y > lista.y + lista.h {
                continue;
            }
            let linha = Rect::new(lista.x, y, lista.w, LINHA - 3.0);
            let sobre = linha.contains(mouse) && lista.contains(mouse);
            realce(linha, self.sel_grupo == Some((g.item_id, g.tier)), sobre);
            crate::bolsa::icone_do_item(Rect::new(linha.x + 4.0, linha.y + 4.0, 34.0, 34.0), g.item_id, 1.0);
            let cor = if g.fundivel() { estilo::TEXTO } else { estilo::SUAVE };
            estilo::texto_ajustado(&nome(g.item_id), linha.x + 44.0, linha.y + 18.0, linha.w - 100.0, 15, cor);
            estilo::texto(
                linha.x + 44.0,
                linha.y + 35.0,
                &format!("Tier {} · {} na bolsa", romano(g.tier), g.slots.len()),
                12,
                estilo::SUAVE,
            );
            if g.fundivel() && motivo_aprimorar(g, slots).is_none() {
                estilo::texto(linha.x + linha.w - 48.0, linha.y + 26.0, "pronto", 12, VERDE);
            }
            if sobre && clicou {
                self.sel_grupo = Some((g.item_id, g.tier));
            }
        }
        let g = gs.iter().find(|g| Some((g.item_id, g.tier)) == self.sel_grupo)?;
        // Detalhe: duas pecas → uma do tier de cima.
        let novo = (g.tier + 1).min(forja::APRIMORAR_TIER_MAX);
        estilo::texto_ajustado(&nome(g.item_id), d.x + 6.0, d.y + 22.0, d.w - 12.0, 19, estilo::OURO);
        let cy = d.y + 44.0;
        for k in 0..2 {
            let r = Rect::new(d.x + 6.0 + k as f32 * 70.0, cy, 60.0, 60.0);
            let tem_peca = g.slots.len() > k;
            crate::bolsa::icone_do_item(r, g.item_id, if tem_peca { 1.0 } else { 0.25 });
            let rot = format!("T {}", romano(g.tier));
            estilo::texto(r.x + 2.0, r.y + r.h + 16.0, &rot, 13, estilo::SUAVE);
        }
        seta(d.x + 146.0, cy + 30.0);
        let r = Rect::new(d.x + 190.0, cy, 60.0, 60.0);
        crate::bolsa::icone_do_item(r, g.item_id, 1.0);
        estilo::texto(r.x + 2.0, r.y + r.h + 16.0, &format!("T {}", romano(novo)), 13, VERDE);
        estilo::texto(
            d.x + 6.0,
            d.y + 150.0,
            "Duas peças iguais viram uma do tier de cima,",
            13,
            estilo::SUAVE,
        );
        estilo::texto(
            d.x + 6.0,
            d.y + 167.0,
            "com os atributos rolados de novo, mais fortes.",
            13,
            estilo::SUAVE,
        );
        let perdido = refino_perdido(g, slots);
        if perdido > 0 && g.fundivel() {
            estilo::texto(
                d.x + 6.0,
                d.y + 188.0,
                &format!("O refino +{perdido} das peças se perde."),
                14,
                AMARELO,
            );
        }
        let cobre = forja::custo_de_aprimorar(g.tier);
        linha_de_custo(
            d,
            d.y + 204.0,
            item_id::COPPER,
            &nome(item_id::COPPER),
            tem(slots, item_id::COPPER),
            cobre,
            &mut self.onde_obter,
        );
        let m = motivo_aprimorar(g, slots);
        let b = Rect::new(d.x + d.w - 160.0, d.y + d.h - 48.0, 150.0, 38.0);
        if let Some(m) = &m {
            estilo::texto_ajustado(m, d.x + 6.0, b.y - 10.0, d.w - 12.0, 14, VERMELHO);
        }
        if crate::ui::botao(b, "Aprimorar", m.is_none()) {
            return Some(ClientMessage::Aprimorar {
                slot_a: g.slots[0] as u16,
                slot_b: g.slots[1] as u16,
            });
        }
        None
    }

    /// Aba Combinar.
    pub fn combinar(
        &mut self,
        lista: Rect,
        d: Rect,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
    ) -> Option<ClientMessage> {
        let nome = |id: u16| nomes.get(&id).cloned().unwrap_or_else(|| format!("item {id}"));
        let t = |id: u16| tem(slots, id);
        fundo_da_lista(lista);
        let rs = receitas(slots);
        self.rolar(lista, rs.len() as f32 * LINHA);
        if self.sel_receita.is_none() {
            self.sel_receita = rs.first().map(|r| r.entrada);
        }
        let mouse = Vec2::from(mouse_position());
        let clicou = is_mouse_button_pressed(MouseButton::Left);
        for (i, r) in rs.iter().enumerate() {
            let y = lista.y + i as f32 * LINHA - self.rolagem;
            if y + LINHA < lista.y || y > lista.y + lista.h {
                continue;
            }
            let linha = Rect::new(lista.x, y, lista.w, LINHA - 3.0);
            let sobre = linha.contains(mouse) && lista.contains(mouse);
            realce(linha, self.sel_receita == Some(r.entrada), sobre);
            crate::bolsa::icone_do_item(Rect::new(linha.x + 4.0, linha.y + 4.0, 34.0, 34.0), r.entrada, 1.0);
            let pode = combinar::vezes_possiveis(r, &t) > 0;
            let cor = if t(r.entrada) > 0 { estilo::TEXTO } else { estilo::SUAVE };
            estilo::texto_ajustado(&nome(r.entrada), linha.x + 44.0, linha.y + 18.0, linha.w - 100.0, 15, cor);
            estilo::texto(
                linha.x + 44.0,
                linha.y + 35.0,
                &format!("{} por 1 · {}% · tem {}", r.qtd, r.chance, t(r.entrada)),
                12,
                estilo::SUAVE,
            );
            if pode {
                estilo::texto(linha.x + linha.w - 48.0, linha.y + 26.0, "pronto", 12, VERDE);
            }
            if sobre && clicou {
                self.sel_receita = Some(r.entrada);
            }
        }
        let r = rs.iter().find(|r| Some(r.entrada) == self.sel_receita)?;
        // Detalhe: N da cor → 1 da de cima, a chance e o que cada tentativa cobra.
        let cy = d.y + 8.0;
        crate::bolsa::icone_do_item(Rect::new(d.x + 6.0, cy, 56.0, 56.0), r.entrada, 1.0);
        estilo::texto(d.x + 70.0, cy + 36.0, &format!("{}x", r.qtd), 17, estilo::TEXTO);
        seta(d.x + 108.0, cy + 28.0);
        crate::bolsa::icone_do_item(Rect::new(d.x + 146.0, cy, 56.0, 56.0), r.saida, 1.0);
        estilo::texto_ajustado(&nome(r.saida), d.x + 210.0, cy + 24.0, d.w - 216.0, 16, estilo::OURO);
        let (chance, cor) = if r.chance >= 100 {
            ("sempre dá certo".to_string(), VERDE)
        } else {
            (format!("{}% de chance por tentativa", r.chance), AMARELO)
        };
        estilo::texto(d.x + 210.0, cy + 46.0, &chance, 14, cor);
        estilo::texto(d.x + 6.0, d.y + 92.0, "Cada tentativa gasta", 15, estilo::TEXTO);
        let mut y = d.y + 104.0;
        for (id, custo) in [
            (r.entrada, r.qtd),
            (item_id::COPPER, r.cobre),
            (item_id::DARKSTEEL, r.darksteel),
            (item_id::GLITTERING_POWDER, r.po),
        ] {
            if custo == 0 {
                continue;
            }
            linha_de_custo(d, y, id, &nome(id), t(id), custo, &mut self.onde_obter);
            y += 36.0;
        }
        if r.chance < 100 {
            estilo::texto(d.x + 6.0, y + 16.0, "Falhar consome tudo.", 13, estilo::SUAVE);
        }
        let n = combinar::vezes_possiveis(r, &t);
        let b1 = Rect::new(d.x + d.w - 160.0, d.y + d.h - 48.0, 150.0, 38.0);
        let b2 = Rect::new(d.x + d.w - 320.0, d.y + d.h - 48.0, 150.0, 38.0);
        if n == 0 {
            estilo::texto(d.x + 6.0, b1.y - 10.0, "Falta material para uma tentativa.", 14, VERMELHO);
        }
        if crate::ui::botao(b1, "Combinar 1x", n > 0) {
            return Some(ClientMessage::Combinar {
                entrada: r.entrada,
                vezes: 1,
            });
        }
        if n > 1 && crate::ui::botao(b2, &format!("Combinar {n}x"), true) {
            return Some(ClientMessage::Combinar {
                entrada: r.entrada,
                vezes: n as u16,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::ItemInstance;

    fn peca(id: u16, tier: u8, refino: u8) -> InventorySlot {
        let mut i = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        i.rarity = tier;
        i.refinement = refino;
        InventorySlot {
            item_id: id,
            qty: 1,
            instance: Some(i),
        }
    }

    fn material(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn grupos_juntam_iguais_e_escolhem_as_de_menor_refino() {
        let k = item_id::KATANA;
        let slots = vec![
            peca(k, 1, 6),
            material(item_id::COPPER, 10),
            peca(k, 1, 0),
            peca(k, 2, 0),
            peca(k, 1, 2),
        ];
        let gs = grupos(&slots);
        assert_eq!(gs[0], Grupo { item_id: k, tier: 1, slots: vec![2, 4, 0] });
        assert!(gs[0].fundivel());
        assert!(!gs[1].fundivel(), "a Tier II esta' sozinha");
        assert_eq!(refino_perdido(&gs[0], &slots), 2);
        assert_eq!(
            motivo_aprimorar(&gs[0], &slots).as_deref(),
            Some("Faltam 490 de cobre")
        );
    }

    #[test]
    fn combinar_mostra_primeiro_o_que_da_pra_tentar() {
        let slots = vec![material(item_id::HORN, 7), material(item_id::STEEL, 3)];
        let rs = receitas(&slots);
        assert_eq!(rs[0].entrada, item_id::HORN, "chifre: 7 pagam uma");
        assert_eq!(rs[1].entrada, item_id::STEEL, "tem aco, mas falta pra sintese");
        assert_eq!(rs.len(), combinar::receitas().len());
    }

    #[test]
    fn texto_do_resultado_diz_quantas_deram_certo() {
        assert!(texto_do_resultado(3, 1, "Chifre", "").0.contains("1x Chifre"));
        assert!(!texto_do_resultado(3, 0, "Chifre", "").1);
        assert!(texto_do_resultado(0, 0, "", "faltam: x").0.contains("faltam"));
    }
}
