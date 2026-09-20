//! Painel de CRAFT: as receitas por categoria, os ingredientes com
//! tem/precisa e o botao Criar.
//!
//! Depois das categorias, duas abas de oficina (`oficina_ui`): APRIMORAR
//! (duas pecas iguais → tier de cima) e COMBINAR (chave/material → cor de
//! cima).
//!
//! Abre pelo Menu (Oficina → Craft) — nunca por tecla.
//! Nada aqui decide: o servidor confere nivel, materiais e espaco e responde
//! `CraftResultado`; o que o painel mostra de "da'/nao da'" e' so' leitura da
//! bolsa pra nao mandar pedido que vai voltar recusado.
//!
//! API pro HUD: `abrir()`, `fechar()`, `aberto()`, `pega_mouse()`,
//! `define_receitas(..)`, `resultado(..)` e `desenha(..) -> Option<ClientMessage>`.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::protocol::{ClientMessage, CraftRecipeNet};
use shared::receitas::{categoria, nome_da_categoria};
use shared::InventorySlot;

use crate::hud_estilo::{self as estilo, u};

const LARGURA: f32 = 780.0;
const ALTURA: f32 = 520.0;
const LINHA: f32 = 40.0;
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);
const VERMELHO: Color = Color::new(0.90, 0.40, 0.34, 1.0);

/// Abas, na ordem do painel.
const ABAS: [u8; 4] = [
    categoria::ARMA,
    categoria::SECUNDARIA,
    categoria::ARMADURA,
    categoria::ACESSORIO,
];

/// As abas de oficina, depois das categorias.
const ABA_APRIMORAR: usize = ABAS.len();
const ABA_COMBINAR: usize = ABAS.len() + 1;

/// Quanto de `id` (material empilhado, sem instancia) a bolsa tem.
pub fn tem(slots: &[InventorySlot], id: u16) -> u32 {
    slots
        .iter()
        .filter(|s| s.item_id == id && s.instance.is_none() && s.qty > 0)
        .map(|s| s.qty)
        .sum()
}

/// (item, tem, precisa) de cada ingrediente.
pub fn ingredientes(r: &CraftRecipeNet, slots: &[InventorySlot]) -> Vec<(u16, u32, u32)> {
    r.inputs
        .iter()
        .filter(|[id, _]| *id != 0)
        .map(|&[id, q]| (id as u16, tem(slots, id as u16), q))
        .collect()
}

/// "Dá: Ataque +8 · Vida +12" e, quando houver, o nível para usar.
/// Os atributos são valores exatos; sem atributos (selo), mostra o item.
pub fn o_que_da(
    r: &CraftRecipeNet,
    faixas: &[(&str, i32, i32)],
    nome_da_saida: &str,
) -> (String, Option<String>) {
    if faixas.is_empty() || !r.roll_instance {
        let qtd = r.output_qty.max(1);
        return (format!("Cria: {qtd}× {nome_da_saida}"), None);
    }
    let atributos: Vec<String> = faixas
        .iter()
        .map(|(n, a, b)| if a == b { format!("{n} +{a}") } else { format!("{n} +{a}–{b}") })
        .collect();
    let mut extra = Vec::new();
    if r.output_item_level > 5 {
        extra.push(format!("usar a partir do Nv {}", r.output_item_level / 2));
    }
    (
        format!("Dá: {}", atributos.join(" · ")),
        (!extra.is_empty()).then(|| extra.join(" · ")),
    )
}

/// Por que nao da' pra criar agora (`None` = da').
pub fn motivo(r: &CraftRecipeNet, slots: &[InventorySlot], nivel: u32) -> Option<String> {
    if nivel < r.nivel_min as u32 {
        return Some(format!("Requer nível {}", r.nivel_min));
    }
    let faltam = ingredientes(r, slots)
        .iter()
        .filter(|(_, t, p)| t < p)
        .count();
    (faltam > 0).then(|| format!("Faltam {faltam} ingrediente(s)"))
}

#[derive(Default)]
pub struct Craft {
    aberto: bool,
    receitas: Vec<CraftRecipeNet>,
    aba: usize,
    sel: Option<u16>,
    rolagem: crate::rolagem::Rolagem,
    /// (texto, deu certo, quando).
    aviso: Option<(String, bool, f64)>,
    /// Lupa tocada num ingrediente: o item pro "Onde obter".
    pub onde_obter: Option<u16>,
    oficina: crate::oficina_ui::Oficina,
}

impl Craft {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn alterna(&mut self) {
        self.aberto = !self.aberto;
    }

    pub fn aberto(&self) -> bool {
        self.aberto
    }

    /// Abre direto numa receita (o "Abrir" do Onde obter).
    pub fn abrir_receita(&mut self, id: u16) {
        self.aberto = true;
        if let Some(r) = self.receitas.iter().find(|r| r.id == id) {
            if let Some(i) = ABAS.iter().position(|c| *c == r.category) {
                self.aba = i;
            }
            self.sel = Some(id);
            self.rolagem.zera();
        }
    }

    /// Lupa tocada em qualquer aba (receita ou oficina).
    pub fn onde_obter(&mut self) -> Option<u16> {
        self.onde_obter.take().or_else(|| self.oficina.onde_obter.take())
    }

    pub fn define_receitas(&mut self, v: Vec<CraftRecipeNet>) {
        self.receitas = v;
    }

    /// `CraftResultado` do servidor.
    pub fn resultado(&mut self, ok: bool, texto: String, agora: f64) {
        self.aviso = Some((texto, ok, agora));
    }

    /// A escala do painel (no celular ele enche a tela; ver
    /// `hud_estilo::escala_do_painel`).
    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, ALTURA)
    }

    fn painel() -> Rect {
        let k = Self::escala();
        let (w, h) = (LARGURA * k, ALTURA * k);
        Rect::new((screen_width() - w) * 0.5, (screen_height() - h) * 0.5, w, h)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// As receitas da aba atual, em ordem de nivel e nome.
    pub fn da_aba(&self) -> Vec<&CraftRecipeNet> {
        let cat = ABAS[self.aba.min(ABAS.len() - 1)];
        let mut v: Vec<&CraftRecipeNet> =
            self.receitas.iter().filter(|r| r.category == cat).collect();
        v.sort_by_key(|r| (r.nivel_min, r.tier, r.id));
        v
    }

    /// Desenha e devolve o pedido do quadro (o `Craft`).
    pub fn desenha(
        &mut self,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        nivel: u32,
        agora: f64,
    ) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(slots, nomes, nivel, agora))
    }

    fn desenha_na_escala(
        &mut self,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        nivel: u32,
        agora: f64,
    ) -> Option<ClientMessage> {
        let linha_h = u(LINHA);
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto(p.x + u(18.0), p.y + u(32.0), "Craft", 22, estilo::OURO);
        let dica = match self.aba {
            ABA_APRIMORAR => "duas iguais sobem o tier; duas Tier IV +8 sobem a cor",
            ABA_COMBINAR => "chave e material de uma cor tentam a cor de cima",
            _ => "chave + materiais da cor + darksteel + cobre",
        };
        estilo::texto(p.x + u(90.0), p.y + u(31.0), dica, 13, estilo::SUAVE);
        if crate::ui::botao(
            Rect::new(p.x + p.w - u(44.0), p.y + u(10.0), u(32.0), u(28.0)),
            "x",
            true,
        ) {
            self.aberto = false;
            return None;
        }
        let nome = |id: u16| {
            nomes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("item {id}"))
        };
        // Abas: as categorias de receita e depois as duas de oficina.
        let mut x = p.x + u(16.0);
        let rotulos = ABAS
            .iter()
            .map(|&c| nome_da_categoria(c))
            .chain(["Aprimorar", "Combinar"]);
        for (i, rot) in rotulos.enumerate() {
            let w = estilo::medir(rot, 15) + u(26.0);
            let r = Rect::new(x, p.y + u(46.0), w, u(28.0));
            if i == self.aba {
                draw_rectangle(
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                    Color::new(estilo::OURO.r, estilo::OURO.g, estilo::OURO.b, 0.22),
                );
            }
            if crate::ui::botao(r, rot, true) && i != self.aba {
                self.aba = i;
                self.sel = None;
                self.rolagem.zera();
                self.oficina.trocou_de_aba();
            }
            x += w + u(6.0);
        }
        // Lista.
        let lista = Rect::new(p.x + u(12.0), p.y + u(84.0), u(330.0), p.h - u(96.0));
        let d = Rect::new(p.x + u(356.0), p.y + u(84.0), p.w - u(368.0), p.h - u(96.0));
        if self.aba >= ABA_APRIMORAR {
            let pedido = if self.aba == ABA_APRIMORAR {
                self.oficina.aprimorar(lista, d, slots, nomes, nivel)
            } else {
                self.oficina.combinar(lista, d, slots, nomes)
            };
            self.desenha_aviso(p, agora);
            return pedido;
        }
        draw_rectangle(
            lista.x,
            lista.y,
            lista.w,
            lista.h,
            Color::new(0.0, 0.0, 0.0, 0.25),
        );
        let mouse = Vec2::from(mouse_position());
        let receitas: Vec<CraftRecipeNet> = self.da_aba().into_iter().cloned().collect();
        let total = receitas.len() as f32 * linha_h;
        let clique = self.rolagem.quadro(lista, total, linha_h);
        let arrastando = self.rolagem.arrastando();
        if receitas.is_empty() {
            estilo::texto(
                lista.x + u(10.0),
                lista.y + u(24.0),
                "Nenhuma receita nesta aba.",
                14,
                estilo::SUAVE,
            );
        }
        if self.sel.is_none() {
            self.sel = receitas.first().map(|r| r.id);
        }
        crate::rolagem::recortar(Some(lista));
        for (i, r) in receitas.iter().enumerate() {
            let y = lista.y + i as f32 * linha_h - self.rolagem.pos;
            if y + linha_h < lista.y || y > lista.y + lista.h {
                continue;
            }
            let linha = Rect::new(lista.x, y, lista.w - u(12.0), linha_h - u(3.0));
            let sobre = !arrastando && linha.contains(mouse) && lista.contains(mouse);
            let marcada = self.sel == Some(r.id);
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
            crate::bolsa::icone_do_item(
                Rect::new(linha.x + u(4.0), linha.y + u(3.0), u(32.0), u(32.0)),
                r.output_item_id,
                1.0,
            );
            let pode = motivo(r, slots, nivel).is_none();
            let cor = if nivel < r.nivel_min as u32 {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            };
            estilo::texto_ajustado(
                &r.name,
                linha.x + u(42.0),
                linha.y + u(17.0),
                linha.w - u(90.0),
                15,
                cor,
            );
            estilo::texto(
                linha.x + u(42.0),
                linha.y + u(33.0),
                &format!("Nv {}", r.nivel_min.max(1)),
                12,
                estilo::SUAVE,
            );
            if pode {
                estilo::texto(
                    linha.x + linha.w - u(44.0),
                    linha.y + u(24.0),
                    "pronto",
                    12,
                    VERDE,
                );
            }
            if clique.is_some_and(|c| linha.contains(c)) {
                self.sel = Some(r.id);
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(lista, total);
        // Detalhe.
        let mut pedido = None;
        if let Some(r) = receitas.iter().find(|r| Some(r.id) == self.sel) {
            crate::bolsa::icone_do_item(
                Rect::new(d.x + u(6.0), d.y + u(4.0), u(56.0), u(56.0)),
                r.output_item_id,
                1.0,
            );
            estilo::texto_ajustado(
                &r.name,
                d.x + u(72.0),
                d.y + u(26.0),
                d.w - u(80.0),
                19,
                estilo::OURO,
            );
            estilo::texto(
                d.x + u(72.0),
                d.y + u(48.0),
                &format!(
                    "{} · nível mínimo {}",
                    nome_da_categoria(r.category),
                    r.nivel_min.max(1)
                ),
                14,
                estilo::SUAVE,
            );
            // O que a peça dá: os valores fixos, pela mesma conta do servidor.
            let faixas = shared::items::faixas_do_roll(r.output_item_id, r.output_item_level);
            let (texto_da, extra) = o_que_da(r, &faixas, &nome(r.output_item_id));
            estilo::texto_ajustado(&texto_da, d.x + u(6.0), d.y + u(80.0), d.w - u(12.0), 15, VERDE);
            if let Some(e) = &extra {
                estilo::texto_ajustado(e, d.x + u(6.0), d.y + u(100.0), d.w - u(12.0), 13, estilo::SUAVE);
            }
            estilo::texto(d.x + u(6.0), d.y + u(128.0), "Ingredientes", 15, estilo::TEXTO);
            for (i, (id, t, q)) in ingredientes(r, slots).into_iter().enumerate() {
                let y = d.y + u(138.0) + i as f32 * u(38.0);
                crate::bolsa::icone_do_item(Rect::new(d.x + u(6.0), y, u(30.0), u(30.0)), id, 1.0);
                estilo::texto_ajustado(
                    &nome(id),
                    d.x + u(44.0),
                    y + u(20.0),
                    d.w - u(210.0),
                    15,
                    estilo::TEXTO,
                );
                let txt = format!("{t}/{q}");
                let cor = if t >= q { VERDE } else { VERMELHO };
                estilo::texto(
                    d.x + d.w - estilo::medir(&txt, 15) - u(50.0),
                    y + u(20.0),
                    &txt,
                    15,
                    cor,
                );
                // Onde obter: a lupa de cada ingrediente.
                if crate::onde_obter::botao(Rect::new(d.x + d.w - u(42.0), y - u(2.0), u(38.0), u(36.0))) {
                    self.onde_obter = Some(id);
                }
            }
            let m = motivo(r, slots, nivel);
            let b = Rect::new(d.x + d.w - u(160.0), d.y + d.h - u(48.0), u(150.0), u(38.0));
            if let Some(m) = &m {
                estilo::texto(d.x + u(6.0), b.y + u(24.0), m, 14, VERMELHO);
            }
            if crate::ui::botao(b, "Criar", m.is_none()) {
                pedido = Some(ClientMessage::Craft { recipe_id: r.id });
            }
        }
        self.desenha_aviso(p, agora);
        pedido
    }

    fn desenha_aviso(&self, p: Rect, agora: f64) {
        if let Some((txt, ok, t)) = &self.aviso {
            if agora - t < 4.0 {
                let cor = if *ok { VERDE } else { VERMELHO };
                estilo::texto_centro(p.x + p.w * 0.5, p.y + p.h - u(8.0), txt, 15, cor);
            }
        }
    }
}

/// Capturas das abas Aprimorar e Combinar com uma bolsa ficticia, sem rede
/// (`MMO_PREVIA_OFICINA=1`; PNGs em `MMO_PREVIA_SAIDA`).
#[cfg(debug_assertions)]
pub async fn previa() {
    use shared::item_id;
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-oficina-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    // O painel centra pela tela: o alvo tem que ter o tamanho dela.
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let peca = |id: u16, cor: u8, tier: u8, refino: u8| {
        let mut i = shared::ItemInstance::roll_for(id, 5, || 0.5).unwrap();
        i.rarity = cor;
        i.tier = tier;
        i.refinement = refino;
        InventorySlot {
            item_id: id,
            qty: 1,
            instance: Some(i),
        }
    };
    let mat = |id: u16, qty: u32| InventorySlot {
        item_id: id,
        qty,
        instance: None,
    };
    let slots = vec![
        peca(item_id::KATANA, 1, 4, 8),
        peca(item_id::KATANA, 1, 4, 9),
        peca(item_id::KATANA, 1, 2, 0),
        peca(item_id::PISTOLAS, 1, 1, 0),
        peca(item_id::PISTOLAS, 1, 1, 3),
        mat(item_id::COPPER, 25_000),
        mat(item_id::HORN, 12),
        mat(item_id::SCALE, 3),
        mat(item_id::STEEL, 45),
        mat(item_id::DARKSTEEL, 1_800),
        mat(item_id::GLITTERING_POWDER, 3),
    ];
    let mut nomes = HashMap::new();
    for (id, n) in [
        (item_id::KATANA, "Katana"),
        (item_id::PISTOLAS, "Pistolas"),
        (item_id::COPPER, "Cobre"),
        (item_id::HORN, "Chifre"),
        (item_id::na_cor(item_id::HORN, 2), "Chifre Verde"),
        (item_id::SCALE, "Escama"),
        (item_id::STEEL, "Aço"),
        (item_id::na_cor(item_id::STEEL, 2), "Aço Verde"),
        (item_id::DARKSTEEL, "Darksteel"),
        (item_id::GLITTERING_POWDER, "Pó Cintilante"),
    ] {
        nomes.insert(id, n.to_string());
    }
    let mut c = Craft::default();
    c.abrir();
    for (aba, nome) in [(ABA_APRIMORAR, "aprimorar"), (ABA_COMBINAR, "combinar")] {
        c.aba = aba;
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            c.desenha(&slots, &nomes, 16, 0.0);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn motivo_diz_nivel_e_faltas_e_libera_com_tudo() {
        let r = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .next()
            .unwrap();
        let tudo: Vec<InventorySlot> = r.inputs.iter().map(|&[id, q]| slot(id as u16, q)).collect();
        assert_eq!(motivo(&r, &tudo, 1), None);
        let mut falta = tudo.clone();
        falta[0].qty = 0;
        falta[5].qty = 1;
        assert_eq!(
            motivo(&r, &falta, 1).as_deref(),
            Some("Faltam 2 ingrediente(s)")
        );
        let epico = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|r| r.nivel_min >= 60)
            .unwrap();
        assert_eq!(
            motivo(&epico, &tudo, 10).as_deref(),
            Some("Requer nível 60")
        );
        let ing = ingredientes(&r, &falta);
        assert_eq!(ing[0], (r.inputs[0][0] as u16, 0, 1));
    }

    #[test]
    fn o_que_da_mostra_o_valor_fixo_de_cada_atributo() {
        let r = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|r| r.output_item_id == shared::item_id::KATANA)
            .unwrap();
        let f = shared::items::faixas_do_roll(r.output_item_id, r.output_item_level);
        let (da, extra) = o_que_da(&r, &f, "Katana");
        assert!(da.starts_with("Dá: Ataque +"), "{da}");
        assert!(da.contains("Destreza +"), "{da}");
        assert!(!da.contains('–'), "não mostra faixa: {da}");
        assert!(extra.as_deref().is_none_or(|e| !e.contains("aleat")));
    }

    #[test]
    fn abrir_receita_vai_pra_aba_e_seleciona() {
        let mut c = Craft::default();
        c.define_receitas(shared::receitas::receitas_de_equipamento());
        let brinco = c
            .receitas
            .iter()
            .find(|r| r.category == categoria::ACESSORIO)
            .unwrap()
            .id;
        c.abrir_receita(brinco);
        assert!(c.aberto());
        assert_eq!((ABAS[c.aba], c.sel), (categoria::ACESSORIO, Some(brinco)));
    }

    #[test]
    fn aba_filtra_por_categoria_e_ordena_por_nivel() {
        let mut c = Craft::default();
        c.define_receitas(shared::receitas::receitas_de_equipamento());
        let armas = c.da_aba();
        assert_eq!(armas.len(), 16, "4 armas x 4 cores");
        assert!(armas.iter().all(|r| r.category == categoria::ARMA));
        assert!(armas.windows(2).all(|w| w[0].nivel_min <= w[1].nivel_min));
        c.aba = 3;
        assert!(c
            .da_aba()
            .iter()
            .all(|r| r.category == categoria::ACESSORIO));
    }
}
