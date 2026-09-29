//! The Craft panel's two workshop tabs: UPGRADE (two identical pieces become
//! one of the next tier) and COMBINE (a key/material of one color tries for
//! the color above).
//!
//! Same rule as the rest of the panel: nothing here decides. The server
//! checks and rolls (`Aprimorar` -> `AprimorarResultado`, `Combinar` ->
//! `CombinarResultado`); the panel only reads the bag to show what is
//! possible and not send a request that comes back refused.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::combinar::{self, ReceitaDeCombinar};
use shared::forja;
use shared::protocol::ClientMessage;
use shared::{item_id, InventorySlot};

use crate::craft_ui::tem;
use crate::hud_estilo::{self as estilo, u};
use crate::vox::VoxCache;

const LINHA: f32 = 44.0;
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);
const VERMELHO: Color = Color::new(0.90, 0.40, 0.34, 1.0);
const AMARELO: Color = Color::new(0.95, 0.78, 0.30, 1.0);
const ROMANO: [&str; 5] = ["I", "II", "III", "IV", "V"];

pub fn romano(tier: u8) -> &'static str {
    ROMANO[(tier.clamp(1, 5) - 1) as usize]
}

// ─────────────────────────────── aprimorar ───────────────────────────────

/// Pecas iguais da bolsa: mesmo item, mesma cor, mesmo tier. `slots` em
/// ordem de refino crescente.
#[derive(Debug, Clone, PartialEq)]
pub struct Grupo {
    pub item_id: u16,
    pub grau: u8,
    pub tier: u8,
    pub slots: Vec<usize>,
}

fn refino(slots: &[InventorySlot], i: usize) -> u8 {
    slots[i].instance.map_or(0, |x| x.refinement)
}

impl Grupo {
    /// Tier IV: a proxima fusao sobe de COR.
    pub fn sobe_de_cor(&self) -> bool {
        self.tier >= forja::TIER_MAX
    }

    /// The two that go into the fusion: the ones with the least refinement (less
    /// is lost); on a color step up, only among the +8s.
    pub fn escolhidas(&self, slots: &[InventorySlot]) -> Vec<usize> {
        self.slots
            .iter()
            .copied()
            .filter(|&i| !self.sobe_de_cor() || refino(slots, i) >= forja::REFINO_PARA_COR)
            .take(2)
            .collect()
    }

    /// Can it be fused (not counting copper and level)?
    pub fn fundivel(&self, slots: &[InventorySlot]) -> bool {
        self.escolhidas(slots).len() == 2 && !(self.sobe_de_cor() && self.grau >= 5)
    }
}

pub fn nome_da_cor(grau: u8) -> &'static str {
    forja::Grau::de_u8(grau).map_or("Common", |g| g.nome())
}

/// The bag's equipment pieces grouped for the tab: the ones that fuse first,
/// then the ones still waiting for a matching second. A piece with a gem is
/// left out (the server refuses).
pub fn grupos(slots: &[InventorySlot]) -> Vec<Grupo> {
    let mut v: Vec<Grupo> = Vec::new();
    for (i, s) in slots.iter().enumerate() {
        let Some(inst) = s.instance.filter(|_| s.qty > 0) else {
            continue;
        };
        if inst.socketed_gems.iter().any(|g| *g != 0) {
            continue;
        }
        let (grau, tier) = (inst.grau(), inst.tier());
        match v
            .iter_mut()
            .find(|g| g.item_id == s.item_id && g.grau == grau && g.tier == tier)
        {
            Some(g) => g.slots.push(i),
            None => v.push(Grupo {
                item_id: s.item_id,
                grau,
                tier,
                slots: vec![i],
            }),
        }
    }
    for g in &mut v {
        g.slots.sort_by_key(|&i| (refino(slots, i), i));
    }
    v.sort_by_key(|g| (!g.fundivel(slots), g.item_id, g.grau, g.tier));
    v
}

/// The highest refinement the group's fusion throws away (of the two chosen).
pub fn refino_perdido(g: &Grupo, slots: &[InventorySlot]) -> u8 {
    g.escolhidas(slots)
        .iter()
        .map(|&i| refino(slots, i))
        .max()
        .unwrap_or(0)
}

/// (color, tier) that comes out of the group's fusion.
pub fn resultado(g: &Grupo) -> (u8, u8) {
    if g.sobe_de_cor() {
        ((g.grau + 1).min(5), 1)
    } else {
        (g.grau, g.tier + 1)
    }
}

/// Why the group does not fuse right now (`None` = it fuses).
pub fn motivo_aprimorar(g: &Grupo, slots: &[InventorySlot], nivel: u32) -> Option<String> {
    if g.sobe_de_cor() && g.grau >= 5 {
        return Some("Legendary IV is the top".into());
    }
    if g.escolhidas(slots).len() < 2 {
        if g.sobe_de_cor() {
            let n = g
                .slots
                .iter()
                .filter(|&&i| refino(slots, i) >= forja::REFINO_PARA_COR)
                .count();
            return Some(format!(
                "Para subir de cor: duas Tier IV +{} (você tem {n})",
                forja::REFINO_PARA_COR
            ));
        }
        return Some("Precisa de mais uma peça igual (mesmo item, cor e tier)".into());
    }
    let (grau, _) = resultado(g);
    if grau > g.grau && nivel < forja::nivel_da_cor(grau) {
        return Some(format!(
            "Requires level {} for {}",
            forja::nivel_da_cor(grau),
            nome_da_cor(grau)
        ));
    }
    let cobre = forja::custo_de_aprimorar(g.grau, g.tier);
    let t = tem(slots, item_id::COPPER);
    (t < cobre).then(|| format!("Missing {} copper", cobre - t))
}

// ─────────────────────────────── combinar ────────────────────────────────

/// The recipes in the tab's order: first the ones that can be tried, then the
/// ones you have something for, then the rest (to show the path). Within each
/// band, keys before materials and ascending color.
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
        return (format!("Not combined: {motivo}"), false);
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

pub fn texto_da_sintese(
    tentativas: u16,
    criados: u16,
    saida: &str,
    motivo: &str,
) -> (String, bool) {
    if tentativas == 0 {
        (format!("Not created: {motivo}"), false)
    } else {
        (format!("Material craft: {criados}x {saida}!"), true)
    }
}

// ────────────────────────────── desenho ─────────────────────────────────

#[derive(Default)]
pub struct Oficina {
    /// Grupo escolhido no Aprimorar: (item, cor, tier).
    sel_grupo: Option<(u16, u8, u8)>,
    /// Receita escolhida no Combinar (a entrada).
    sel_receita: Option<u16>,
    /// Combine: show only what can be tried right now. The list has one recipe
    /// per material of each color — there are dozens, and almost always the
    /// player wants to see the four they can make today.
    so_possiveis: bool,
    rolagem: crate::rolagem::Rolagem,
    /// Magnifier touched: the item for "Where to get".
    pub onde_obter: Option<u16>,
}

/// An arrow that "becomes" between two icons. Drawn: the HUD font has no "->".
fn seta(x: f32, y: f32) {
    let c = estilo::OURO;
    draw_rectangle(x, y - u(3.0), u(18.0), u(6.0), c);
    draw_triangle(
        Vec2::new(x + u(16.0), y - u(10.0)),
        Vec2::new(x + u(16.0), y + u(10.0)),
        Vec2::new(x + u(28.0), y),
        c,
    );
}

/// A cor de um grau (a mesma da borda da bolsa).
fn cor_da_cor(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
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

/// A row "icon  name ........ have/need" with the Where to get magnifier.
fn linha_de_custo(
    d: Rect,
    y: f32,
    id: u16,
    nome: &str,
    t: u32,
    precisa: u32,
    onde: &mut Option<u16>,
    palco: Option<(&VoxCache, &Material)>,
) {
    crate::bolsa::icone_do_item_com(Rect::new(d.x + u(6.0), y, u(30.0), u(30.0)), id, 1.0, palco);
    estilo::texto_ajustado(
        nome,
        d.x + u(44.0),
        y + u(20.0),
        d.w - u(210.0),
        15,
        estilo::TEXTO,
    );
    let txt = format!(
        "{}/{}",
        crate::bolsa::milhar(t as u64),
        crate::bolsa::milhar(precisa as u64)
    );
    let cor = if t >= precisa { VERDE } else { VERMELHO };
    estilo::texto(
        d.x + d.w - estilo::medir(&txt, 15) - u(50.0),
        y + u(20.0),
        &txt,
        15,
        cor,
    );
    if crate::onde_obter::botao(Rect::new(d.x + d.w - u(42.0), y - u(2.0), u(38.0), u(36.0))) {
        *onde = Some(id);
    }
}

impl Oficina {
    pub fn seleciona_grupo(&mut self, item_id: u16, grau: u8, tier: u8) {
        self.sel_grupo = Some((item_id, grau, tier));
        self.rolagem.zera();
    }
    pub fn seleciona_receita(&mut self, entrada: u16) {
        self.sel_receita = Some(entrada);
        self.rolagem.zera();
    }

    pub fn trocou_de_aba(&mut self) {
        self.rolagem.zera();
    }

    /// Upgrade tab. `lista` and `d` are the panel's two columns.
    pub fn aprimorar(
        &mut self,
        lista: Rect,
        d: Rect,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        nivel: u32,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        let nome = |id: u16| {
            nomes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("item {id}"))
        };
        let chave = |g: &Grupo| (g.item_id, g.grau, g.tier);
        fundo_da_lista(lista);
        let gs = grupos(slots);
        let total = gs.len() as f32 * u(LINHA);
        let clique = self.rolagem.quadro(lista, total, u(LINHA));
        let arrastando = self.rolagem.arrastando();
        if gs.is_empty() {
            estilo::texto(
                lista.x + u(10.0),
                lista.y + u(24.0),
                "No piece in your bag.",
                14,
                estilo::SUAVE,
            );
            estilo::texto(
                lista.x + u(10.0),
                lista.y + u(44.0),
                "Crie no Craft ou guarde as que caírem.",
                13,
                estilo::SUAVE,
            );
        }
        if self
            .sel_grupo
            .is_none_or(|s| !gs.iter().any(|g| chave(g) == s))
        {
            self.sel_grupo = gs.first().map(chave);
        }
        let mouse = Vec2::from(mouse_position());
        crate::rolagem::recortar(Some(lista));
        for (i, g) in gs.iter().enumerate() {
            let y = lista.y + i as f32 * u(LINHA) - self.rolagem.pos;
            if y + u(LINHA) < lista.y || y > lista.y + lista.h {
                continue;
            }
            let linha = Rect::new(lista.x, y, lista.w - u(12.0), u(LINHA) - u(3.0));
            let sobre = !arrastando && linha.contains(mouse) && lista.contains(mouse);
            realce(linha, self.sel_grupo == Some(chave(g)), sobre);
            crate::bolsa::icone_do_item_com(
                Rect::new(linha.x + u(4.0), linha.y + u(4.0), u(34.0), u(34.0)),
                g.item_id,
                1.0,
                palco,
            );
            let cor = if g.fundivel(slots) {
                estilo::TEXTO
            } else {
                estilo::SUAVE
            };
            estilo::texto_ajustado(
                &nome(g.item_id),
                linha.x + u(44.0),
                linha.y + u(18.0),
                linha.w - u(100.0),
                15,
                cor,
            );
            estilo::texto(
                linha.x + u(44.0),
                linha.y + u(35.0),
                &format!(
                    "{} · Tier {} · {} in your bag",
                    nome_da_cor(g.grau),
                    romano(g.tier),
                    g.slots.len()
                ),
                12,
                cor_da_cor(g.grau),
            );
            if motivo_aprimorar(g, slots, nivel).is_none() {
                estilo::texto(
                    linha.x + linha.w - u(48.0),
                    linha.y + u(26.0),
                    "pronto",
                    12,
                    VERDE,
                );
            }
            if clique.is_some_and(|c| linha.contains(c)) {
                self.sel_grupo = Some(chave(g));
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(lista, total);
        let g = gs.iter().find(|g| Some(chave(g)) == self.sel_grupo)?;
        // Detail: two pieces -> one of the step above.
        let (grau_novo, tier_novo) = resultado(g);
        let escolhidas = g.escolhidas(slots);
        let titulo = if g.sobe_de_cor() {
            format!(
                "{} · {} to {}",
                nome(g.item_id),
                nome_da_cor(g.grau),
                nome_da_cor(grau_novo)
            )
        } else {
            format!("{} · {}", nome(g.item_id), nome_da_cor(g.grau))
        };
        estilo::texto_ajustado(
            &titulo,
            d.x + u(6.0),
            d.y + u(22.0),
            d.w - u(12.0),
            19,
            estilo::OURO,
        );
        let cy = d.y + u(44.0);
        for k in 0..2 {
            let r = Rect::new(d.x + u(6.0) + k as f32 * u(70.0), cy, u(60.0), u(60.0));
            let tem_peca = escolhidas.len() > k;
            crate::bolsa::icone_do_item_com(r, g.item_id, if tem_peca { 1.0 } else { 0.25 }, palco);
            let mut rot = format!("T {}", romano(g.tier));
            if let Some(&i) = escolhidas.get(k) {
                if refino(slots, i) > 0 {
                    rot = format!("{rot} +{}", refino(slots, i));
                }
            }
            estilo::texto(r.x, r.y + r.h + u(16.0), &rot, 12, cor_da_cor(g.grau));
        }
        seta(d.x + u(152.0), cy + u(30.0));
        let r = Rect::new(d.x + u(196.0), cy, u(60.0), u(60.0));
        crate::bolsa::icone_do_item_com(r, g.item_id, 1.0, palco);
        estilo::texto(
            r.x,
            r.y + r.h + u(16.0),
            &format!("{} {}", nome_da_cor(grau_novo), romano(tier_novo)),
            13,
            cor_da_cor(grau_novo),
        );
        let (l1, l2) = if g.sobe_de_cor() {
            (
                format!(
                    "Duas Tier IV +{} viram uma da cor de cima,",
                    forja::REFINO_PARA_COR
                ),
                "no Tier I, com os atributos fixos da nova cor.".to_string(),
            )
        } else {
            (
                "Duas peças iguais viram uma do tier de cima,".to_string(),
                "com os atributos fixos do novo tier, mais fortes.".to_string(),
            )
        };
        estilo::texto(d.x + u(6.0), d.y + u(150.0), &l1, 13, estilo::SUAVE);
        estilo::texto(d.x + u(6.0), d.y + u(167.0), &l2, 13, estilo::SUAVE);
        let perdido = refino_perdido(g, slots);
        if perdido > 0 && g.fundivel(slots) {
            estilo::texto(
                d.x + u(6.0),
                d.y + u(188.0),
                &format!("O refino +{perdido} das peças se perde."),
                14,
                AMARELO,
            );
        }
        let cobre = forja::custo_de_aprimorar(g.grau, g.tier);
        linha_de_custo(
            d,
            d.y + u(204.0),
            item_id::COPPER,
            &nome(item_id::COPPER),
            tem(slots, item_id::COPPER),
            cobre,
            &mut self.onde_obter,
            palco,
        );
        let m = motivo_aprimorar(g, slots, nivel);
        let b = Rect::new(d.x + d.w - u(160.0), d.y + d.h - u(48.0), u(150.0), u(38.0));
        if let Some(m) = &m {
            estilo::texto_ajustado(m, d.x + u(6.0), b.y - u(10.0), d.w - u(12.0), 14, VERMELHO);
        }
        if crate::ui::botao(b, "Upgrade", m.is_none()) {
            return Some(ClientMessage::Aprimorar {
                slot_a: escolhidas[0] as u16,
                slot_b: escolhidas[1] as u16,
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
        materiais: bool,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        let nome = |id: u16| {
            nomes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("item {id}"))
        };
        let t = |id: u16| tem(slots, id);
        // ── filtro: a barra come a primeira faixa da lista ──
        let filtro = Rect::new(lista.x, lista.y, lista.w - u(12.0), u(26.0));
        let lista = Rect::new(
            lista.x,
            lista.y + u(32.0),
            lista.w,
            (lista.h - u(32.0)).max(u(40.0)),
        );
        let todas: Vec<_> = receitas(slots)
            .into_iter()
            .filter(|r| (r.chance == 100) == materiais)
            .collect();
        let quantas_dao = todas
            .iter()
            .filter(|r| combinar::vezes_possiveis(r, &t) > 0)
            .count();
        if crate::ui::botao(
            filtro,
            &if self.so_possiveis {
                format!("showing only what works ({quantas_dao})")
            } else {
                format!("show only what works ({quantas_dao})")
            },
            true,
        ) {
            self.so_possiveis = !self.so_possiveis;
            self.rolagem.zera();
        }
        fundo_da_lista(lista);
        let rs: Vec<ReceitaDeCombinar> = todas
            .into_iter()
            .filter(|r| !self.so_possiveis || combinar::vezes_possiveis(r, &t) > 0)
            .collect();
        if rs.is_empty() {
            estilo::texto_ajustado(
                if materiais {
                    "Nada pronto pra criar agora. Toque de novo pra ver a lista inteira."
                } else {
                    "Nada pronto pra combinar agora. Toque de novo pra ver a lista inteira."
                },
                lista.x + u(10.0),
                lista.y + u(26.0),
                lista.w - u(24.0),
                14,
                estilo::SUAVE,
            );
            self.sel_receita = None;
            return None;
        }
        // The chosen one left the list with the filter: falls to the first one remaining.
        if !rs.iter().any(|r| Some(r.entrada) == self.sel_receita) {
            self.sel_receita = None;
        }
        let total = rs.len() as f32 * u(LINHA);
        let clique = self.rolagem.quadro(lista, total, u(LINHA));
        let arrastando = self.rolagem.arrastando();
        if self.sel_receita.is_none() {
            self.sel_receita = rs.first().map(|r| r.entrada);
        }
        let mouse = Vec2::from(mouse_position());
        crate::rolagem::recortar(Some(lista));
        for (i, r) in rs.iter().enumerate() {
            let y = lista.y + i as f32 * u(LINHA) - self.rolagem.pos;
            if y + u(LINHA) < lista.y || y > lista.y + lista.h {
                continue;
            }
            let linha = Rect::new(lista.x, y, lista.w - u(12.0), u(LINHA) - u(3.0));
            let sobre = !arrastando && linha.contains(mouse) && lista.contains(mouse);
            realce(linha, self.sel_receita == Some(r.entrada), sobre);
            crate::bolsa::icone_do_item_com(
                Rect::new(linha.x + u(4.0), linha.y + u(4.0), u(34.0), u(34.0)),
                r.entrada,
                1.0,
                palco,
            );
            let pode = combinar::vezes_possiveis(r, &t) > 0;
            let cor = if t(r.entrada) > 0 {
                estilo::TEXTO
            } else {
                estilo::SUAVE
            };
            estilo::texto_ajustado(
                &nome(r.entrada),
                linha.x + u(44.0),
                linha.y + u(18.0),
                linha.w - u(100.0),
                15,
                cor,
            );
            estilo::texto(
                linha.x + u(44.0),
                linha.y + u(35.0),
                &if r.chance == 100 {
                    format!("{} for 1 · guaranteed · you have {}", r.qtd, t(r.entrada))
                } else {
                    format!("{} for 1 · {}% · you have {}", r.qtd, r.chance, t(r.entrada))
                },
                12,
                estilo::SUAVE,
            );
            if pode {
                estilo::texto(
                    linha.x + linha.w - u(48.0),
                    linha.y + u(26.0),
                    "pronto",
                    12,
                    VERDE,
                );
            }
            if clique.is_some_and(|c| linha.contains(c)) {
                self.sel_receita = Some(r.entrada);
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(lista, total);
        let r = rs.iter().find(|r| Some(r.entrada) == self.sel_receita)?;
        // Detail: N of the color -> 1 of the one above, the chance and what each attempt costs.
        let cy = d.y + u(8.0);
        crate::bolsa::icone_do_item_com(
            Rect::new(d.x + u(6.0), cy, u(56.0), u(56.0)),
            r.entrada,
            1.0,
            palco,
        );
        estilo::texto(
            d.x + u(70.0),
            cy + u(36.0),
            &format!("{}x", r.qtd),
            17,
            estilo::TEXTO,
        );
        seta(d.x + u(108.0), cy + u(28.0));
        crate::bolsa::icone_do_item_com(
            Rect::new(d.x + u(146.0), cy, u(56.0), u(56.0)),
            r.saida,
            1.0,
            palco,
        );
        estilo::texto_ajustado(
            &nome(r.saida),
            d.x + u(210.0),
            cy + u(24.0),
            d.w - u(216.0),
            16,
            estilo::OURO,
        );
        let (chance, cor) = if r.chance >= 100 {
            ("always succeeds".to_string(), VERDE)
        } else {
            (format!("{}% per attempt", r.chance), AMARELO)
        };
        estilo::texto_ajustado(
            &chance,
            d.x + u(210.0),
            cy + u(46.0),
            d.w - u(216.0),
            14,
            cor,
        );
        estilo::texto(
            d.x + u(6.0),
            d.y + u(92.0),
            if r.chance == 100 {
                "Each synthesis spends"
            } else {
                "Each attempt spends"
            },
            15,
            estilo::TEXTO,
        );
        let mut y = d.y + u(104.0);
        for (id, custo) in [
            (r.entrada, r.qtd),
            (item_id::COPPER, r.cobre),
            (item_id::DARKSTEEL, r.darksteel),
            (item_id::GLITTERING_POWDER, r.po),
        ] {
            if custo == 0 {
                continue;
            }
            linha_de_custo(
                d,
                y,
                id,
                &nome(id),
                t(id),
                custo,
                &mut self.onde_obter,
                palco,
            );
            y += u(36.0);
        }
        if r.chance < 100 {
            estilo::texto(
                d.x + u(6.0),
                y + u(16.0),
                "Failing consumes everything.",
                13,
                estilo::SUAVE,
            );
        }
        let n = combinar::vezes_possiveis(r, &t);
        let b1 = Rect::new(d.x + d.w - u(160.0), d.y + d.h - u(48.0), u(150.0), u(38.0));
        let b2 = Rect::new(d.x + d.w - u(320.0), d.y + d.h - u(48.0), u(150.0), u(38.0));
        if n == 0 {
            estilo::texto(
                d.x + u(6.0),
                b1.y - u(10.0),
                "Not enough material for one attempt.",
                14,
                VERMELHO,
            );
        }
        if crate::ui::botao(
            b1,
            if r.chance == 100 {
                "Synthesise 1x"
            } else {
                "Combine 1x"
            },
            n > 0,
        ) {
            return Some(ClientMessage::Combinar {
                entrada: r.entrada,
                vezes: 1,
            });
        }
        let rotulo_tudo = if r.chance == 100 {
            format!("Synthesise {n}x")
        } else {
            format!("Combine {n}x")
        };
        if n > 1 && crate::ui::botao(b2, &rotulo_tudo, true) {
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

    fn peca(id: u16, cor: u8, tier: u8, refino: u8) -> InventorySlot {
        let mut i = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        i.rarity = cor;
        i.tier = tier;
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
            peca(k, 1, 1, 6),
            material(item_id::COPPER, 10),
            peca(k, 1, 1, 0),
            peca(k, 1, 2, 0),
            peca(k, 1, 1, 2),
            peca(k, 2, 1, 0),
        ];
        let gs = grupos(&slots);
        assert_eq!(
            gs[0],
            Grupo {
                item_id: k,
                grau: 1,
                tier: 1,
                slots: vec![2, 4, 0]
            }
        );
        assert!(gs[0].fundivel(&slots));
        assert!(
            gs[1..].iter().all(|g| !g.fundivel(&slots)),
            "a Tier II e a verde estao sozinhas"
        );
        assert_eq!(refino_perdido(&gs[0], &slots), 2);
        assert_eq!(
            motivo_aprimorar(&gs[0], &slots, 1).as_deref(),
            Some("Faltam 490 de cobre")
        );
    }

    #[test]
    fn tier_iv_so_funde_entre_as_mais_8_e_sobe_de_cor() {
        let k = item_id::KATANA;
        let mut slots = vec![
            peca(k, 1, 4, 3),
            peca(k, 1, 4, 8),
            material(item_id::COPPER, 99_999),
        ];
        let g = grupos(&slots)[0].clone();
        assert!(!g.fundivel(&slots), "so' uma +8");
        assert!(motivo_aprimorar(&g, &slots, 30)
            .unwrap()
            .contains("você tem 1"));
        slots[0] = peca(k, 1, 4, 10);
        let g = grupos(&slots)[0].clone();
        assert_eq!(g.escolhidas(&slots), vec![1, 0]);
        assert_eq!(resultado(&g), (2, 1));
        assert!(motivo_aprimorar(&g, &slots, 19)
            .unwrap()
            .contains("nível 20"));
        assert_eq!(motivo_aprimorar(&g, &slots, 20), None);
    }

    #[test]
    fn combinar_mostra_primeiro_o_que_da_pra_tentar() {
        // A common material appears in the list; whoever can act comes first.
        let slots = vec![material(item_id::HORN, 7), material(item_id::SCALE, 2)];
        let rs = receitas(&slots);
        assert_eq!(rs[0].entrada, item_id::HORN, "chifre: 7 pagam uma");
        assert!(rs.iter().any(|r| r.entrada == item_id::STEEL));
        assert_eq!(rs.len(), combinar::receitas().len());
    }

    /// The Combine filter shows only what can be tried NOW — and the sum that
    /// goes in the label is the same one the filter applies.
    #[test]
    fn o_filtro_do_combinar_deixa_so_o_que_da() {
        let slots = vec![material(item_id::HORN, 7), material(item_id::STEEL, 3)];
        let t = |id: u16| tem(&slots, id);
        let todas = receitas(&slots);
        let dao: Vec<_> = todas
            .iter()
            .filter(|r| combinar::vezes_possiveis(r, &t) > 0)
            .collect();
        assert!(!dao.is_empty(), "com 7 chifres alguma coisa tem que dar");
        assert!(
            dao.len() < todas.len(),
            "se tudo desse, o filtro nao teria pra que existir"
        );
        assert!(
            dao.iter().all(|r| t(r.entrada) >= r.qtd),
            "entrou no filtro sem ter as pecas na mao"
        );
        // An empty bag: the filter leaves nothing, and the screen has to say so
        // instead of showing a blank list.
        let vazia: Vec<InventorySlot> = Vec::new();
        let sem = |id: u16| tem(&vazia, id);
        assert_eq!(
            receitas(&vazia)
                .iter()
                .filter(|r| combinar::vezes_possiveis(r, &sem) > 0)
                .count(),
            0
        );
    }

    #[test]
    fn texto_do_resultado_diz_quantas_deram_certo() {
        assert!(texto_do_resultado(3, 1, "Horn", "")
            .0
            .contains("1x Chifre"));
        assert!(!texto_do_resultado(3, 0, "Horn", "").1);
        assert!(texto_do_resultado(0, 0, "", "faltam: x")
            .0
            .contains("faltam"));
    }
}
