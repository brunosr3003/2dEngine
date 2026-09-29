//! Painel de CRAFT: as receitas por categoria, os ingredientes com
//! tem/precisa e o botao Criar.
//!
//! Depois das categorias: MATERIAIS (sintese 10 para 1), APRIMORAR
//! (duas pecas iguais → tier de cima) e COMBINAR (chaves/pets/montarias).
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
use crate::vox::VoxCache;
use macroquad::material::Material;

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
const ABA_MATERIAIS: usize = ABAS.len();
const ABA_APRIMORAR: usize = ABAS.len() + 1;
const ABA_COMBINAR: usize = ABAS.len() + 2;

/// Quanto de `id` a bolsa tem. Pets e montarias possuem instancia propria,
/// mas contam no Combinar como no servidor.
pub fn tem(slots: &[InventorySlot], id: u16) -> u32 {
    let bicho = shared::pets::de_item(id).is_some() || shared::montarias::de_item(id).is_some();
    slots
        .iter()
        .filter(|s| s.item_id == id && (bicho || s.instance.is_none()) && s.qty > 0)
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
    nivel: u32,
) -> (String, Option<String>) {
    if faixas.is_empty() || !r.roll_instance {
        let qtd = r.output_qty.max(1);
        return (format!("Creates: {qtd}× {nome_da_saida}"), None);
    }
    let mut atributos: Vec<String> = faixas
        .iter()
        .map(|(n, a, b)| {
            if a == b {
                format!("{n} +{a}")
            } else {
                format!("{n} +{a}–{b}")
            }
        })
        .collect();
    // A FOR da armadura MEDIA nao esta' nas faixas: ela entra em
    // `effective_stats` como ponto alocado, e nao como atributo do template.
    // A ficha da bolsa ja' tinha sido corrigida; aqui nao — e o dono leu de
    // novo, montando a peca, que "a media nao da' forca".
    //
    // Ela cresce com o NIVEL DO JOGADOR, entao e' o nivel dele que entra.
    let emp = shared::for_da_armadura(r.output_item_id, nivel.max(1));
    if emp > 0 {
        atributos.push(format!("Strength +{emp}"));
    }
    let mut extra = Vec::new();
    if r.output_item_level > 5 {
        extra.push(format!("usable from Lv {}", r.output_item_level / 2));
    }
    (
        format!("Gives: {}", atributos.join(" · ")),
        (!extra.is_empty()).then(|| extra.join(" · ")),
    )
}

/// Por que nao da' pra criar agora (`None` = da').
pub fn motivo(r: &CraftRecipeNet, slots: &[InventorySlot], nivel: u32) -> Option<String> {
    if nivel < r.nivel_min as u32 {
        return Some(format!("Requires level {}", r.nivel_min));
    }
    let faltam = ingredientes(r, slots)
        .iter()
        .filter(|(_, t, p)| t < p)
        .count();
    (faltam > 0).then(|| format!("Missing {faltam} ingredient(s)"))
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
    /// Aberto pela entrada COMBINAR do menu: sem as abas de categoria.
    so_combinar: bool,
}

impl Craft {
    pub fn receitas_atuais(&self) -> &[CraftRecipeNet] {
        &self.receitas
    }
}

impl Craft {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.so_combinar = false;
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

    /// Abre direto numa receita (o "Open" do Onde obter).
    pub fn abrir_receita(&mut self, id: u16) {
        self.aberto = true;
        self.so_combinar = false;
        if let Some(r) = self.receitas.iter().find(|r| r.id == id) {
            if let Some(i) = ABAS.iter().position(|c| *c == r.category) {
                self.aba = i;
            }
            self.sel = Some(id);
            self.rolagem.zera();
        }
    }

    /// Abre a sintese do material indicada pela lupa de Onde obter.
    pub fn abrir_material(&mut self, entrada: u16) {
        self.aberto = true;
        self.so_combinar = false;
        self.aba = ABA_MATERIAIS;
        self.oficina.seleciona_receita(entrada);
    }

    /// Abre no que este ITEM tem a ver: a receita que o FAZ, ou a primeira
    /// que o gasta.
    ///
    /// Peca primeiro: quem olha uma espada quer melhorar a espada. Material
    /// depois: quem olha madeira quer saber no que ela vira. Sem nenhuma das
    /// duas, abre o Craft do jeito que estava — abrir e' sempre util, e um
    /// atalho que nao faz nada e' pior que nenhum.
    pub fn abrir_pelo_item(&mut self, item_id: u16) {
        if let Some(r) = shared::combinar::receitas()
            .into_iter()
            .find(|r| r.chance == 100 && r.saida == item_id)
            .or_else(|| shared::combinar::receita(item_id).filter(|r| r.chance == 100))
        {
            self.abrir_material(r.entrada);
            return;
        }
        let faz = self.receitas.iter().find(|r| r.output_item_id == item_id);
        let gasta = || {
            self.receitas
                .iter()
                .find(|r| r.inputs.iter().any(|i| i[0] == item_id as u32))
        };
        match faz.or_else(gasta).map(|r| r.id) {
            Some(id) => self.abrir_receita(id),
            None => self.abrir(),
        }
    }

    /// Abre no que DA' PRA FAZER AGORA (a missao "crie um equipamento").
    ///
    /// A missao mandava `abrir()`, que cai na primeira aba — e quase sempre a
    /// receita possivel esta' noutra. O dono: "na missao de craft manda ja'
    /// direto no craft q da' pra fazer pela missao, q e' o de armadura, ja'
    /// abrir direto nessa aba".
    ///
    /// Quem decide o que "da' pra fazer" e' o mesmo `motivo` que pinta o
    /// botao: nivel e ingredientes na mao. Sem nenhuma possivel, abre a mais
    /// barata — ver a que falta menos ensina mais que uma aba vazia.
    pub fn abrir_no_que_da(&mut self, slots: &[InventorySlot], nivel: u32) {
        let possivel = self
            .receitas
            .iter()
            .filter(|r| motivo(r, slots, nivel).is_none())
            .min_by_key(|r| (r.nivel_min, r.id))
            .map(|r| r.id);
        let alcancavel = || {
            self.receitas
                .iter()
                .filter(|r| nivel >= r.nivel_min as u32)
                .min_by_key(|r| (r.nivel_min, r.id))
                .map(|r| r.id)
        };
        match possivel.or_else(alcancavel) {
            Some(id) => self.abrir_receita(id),
            None => self.abrir(),
        }
    }

    /// Abre SO' o Combinar (a entrada propria do menu).
    ///
    /// O dono: "combinar tem q ser uma aba separada no menu, n junto com o
    /// craft". Combinar nao e' receita — e' arrastar cinco iguais e torcer —,
    /// e ficava como a sexta abinha de uma tela de receitas, onde ninguem
    /// achava. Entrando pelo menu, as abas de categoria somem e a tela e' so'
    /// dela.
    pub fn abrir_combinar(&mut self) {
        self.aberto = true;
        self.so_combinar = true;
        self.aba = ABA_COMBINAR;
        self.oficina.trocou_de_aba();
    }

    pub fn abrir_combinacao_do_item(&mut self, item_id: u16) {
        self.abrir_combinar();
        self.oficina.seleciona_receita(item_id);
    }

    pub fn abrir_aprimoramento(&mut self, item_id: u16, grau: u8, tier: u8) {
        self.aberto = true;
        self.so_combinar = false;
        self.aba = ABA_APRIMORAR;
        self.oficina.seleciona_grupo(item_id, grau, tier);
    }

    /// Lupa tocada em qualquer aba (receita ou oficina).
    pub fn onde_obter(&mut self) -> Option<u16> {
        self.onde_obter
            .take()
            .or_else(|| self.oficina.onde_obter.take())
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
        Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        )
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
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || {
            self.desenha_na_escala(slots, nomes, nivel, agora, palco)
        })
    }

    fn desenha_na_escala(
        &mut self,
        slots: &[InventorySlot],
        nomes: &HashMap<u16, String>,
        nivel: u32,
        agora: f64,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        let linha_h = u(LINHA);
        let p = Self::painel();
        estilo::painel(p);
        let titulo = if self.so_combinar {
            "Combine"
        } else {
            "Craft"
        };
        estilo::texto(p.x + u(18.0), p.y + u(32.0), titulo, 22, estilo::OURO);
        let dica = match self.aba {
            ABA_MATERIAIS => "10 materiais + cobre, darksteel e pó criam 1 da cor seguinte",
            ABA_APRIMORAR => "duas peças iguais sobem o tier; duas Tier IV +8 sobem a cor",
            ABA_COMBINAR => "chaves, pets e montarias: 5 para tentar a cor seguinte",
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
        //
        // Entrando pela entrada COMBINAR do menu nao ha' abas nenhuma: a tela
        // e' so' dela, e uma fileira de categorias de receita em cima do
        // arrasta-cinco-iguais so' diria que ele e' um apendice do Craft.
        let mut x = p.x + u(16.0);
        let rotulos: Vec<&str> = if self.so_combinar {
            Vec::new()
        } else {
            ABAS.iter()
                .map(|&c| nome_da_categoria(c))
                .chain(["Materials", "Upgrade", "Combine"])
                .collect()
        };
        for (i, rot) in rotulos.into_iter().enumerate() {
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
        if self.aba >= ABA_MATERIAIS {
            let pedido = if self.aba == ABA_APRIMORAR {
                self.oficina.aprimorar(lista, d, slots, nomes, nivel, palco)
            } else {
                self.oficina
                    .combinar(lista, d, slots, nomes, self.aba == ABA_MATERIAIS, palco)
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
                "No recipe in this tab.",
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
            crate::bolsa::icone_do_item_com(
                Rect::new(linha.x + u(4.0), linha.y + u(3.0), u(32.0), u(32.0)),
                r.output_item_id,
                1.0,
                palco,
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
            crate::bolsa::icone_do_item_com(
                Rect::new(d.x + u(6.0), d.y + u(4.0), u(56.0), u(56.0)),
                r.output_item_id,
                1.0,
                palco,
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
                    "{} · minimum level {}",
                    nome_da_categoria(r.category),
                    r.nivel_min.max(1)
                ),
                14,
                estilo::SUAVE,
            );
            // O que a peça dá: os valores fixos, pela mesma conta do servidor.
            let faixas = shared::items::faixas_do_roll(r.output_item_id, r.output_item_level);
            let (texto_da, extra) = o_que_da(r, &faixas, &nome(r.output_item_id), nivel);
            estilo::texto_ajustado(
                &texto_da,
                d.x + u(6.0),
                d.y + u(80.0),
                d.w - u(12.0),
                15,
                VERDE,
            );
            if let Some(e) = &extra {
                estilo::texto_ajustado(
                    e,
                    d.x + u(6.0),
                    d.y + u(100.0),
                    d.w - u(12.0),
                    13,
                    estilo::SUAVE,
                );
            }
            estilo::texto(
                d.x + u(6.0),
                d.y + u(128.0),
                "Ingredients",
                15,
                estilo::TEXTO,
            );
            for (i, (id, t, q)) in ingredientes(r, slots).into_iter().enumerate() {
                let y = d.y + u(138.0) + i as f32 * u(38.0);
                crate::bolsa::icone_do_item_com(
                    Rect::new(d.x + u(6.0), y, u(30.0), u(30.0)),
                    id,
                    1.0,
                    palco,
                );
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
                if crate::onde_obter::botao(Rect::new(
                    d.x + d.w - u(42.0),
                    y - u(2.0),
                    u(38.0),
                    u(36.0),
                )) {
                    self.onde_obter = Some(id);
                }
            }
            let m = motivo(r, slots, nivel);
            let b = Rect::new(d.x + d.w - u(160.0), d.y + d.h - u(48.0), u(150.0), u(38.0));
            if let Some(m) = &m {
                estilo::texto(d.x + u(6.0), b.y + u(24.0), m, 14, VERMELHO);
            }
            if crate::ui::botao(b, "Create", m.is_none()) {
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
        (item_id::PISTOLAS, "Pistols"),
        (item_id::COPPER, "Copper"),
        (item_id::HORN, "Horn"),
        (item_id::na_cor(item_id::HORN, 2), "Green Horn"),
        (item_id::SCALE, "Scale"),
        (item_id::STEEL, "Steel"),
        (item_id::na_cor(item_id::STEEL, 2), "Green Steel"),
        (item_id::DARKSTEEL, "Darksteel"),
        (item_id::GLITTERING_POWDER, "Shimmering Dust"),
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
            c.desenha(&slots, &nomes, 16, 0.0, None);
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
            Some("Missing 2 ingredient(s)")
        );
        // A MAIS ALTA que existe, e nao ">= 60": o nivel das cores sai de
        // `chaves::FAIXAS` e a epica desceu pro 40 em 28/09/2026.
        let epico = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .max_by_key(|r| r.nivel_min)
            .unwrap();
        let n = epico.nivel_min;
        assert_eq!(
            motivo(&epico, &tudo, n as u32 - 1).as_deref(),
            Some(format!("Requires level {n}").as_str())
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
        let (da, extra) = o_que_da(&r, &f, "Katana", 10);
        assert!(da.starts_with("Gives: Attack +"), "{da}");
        assert!(da.contains("Dexterity +"), "{da}");
        assert!(!da.contains('–'), "não mostra faixa: {da}");
        assert!(extra.as_deref().is_none_or(|e| !e.contains("aleat")));
    }

    /// A armadura MÉDIA diz que dá Força — na tela onde ela é montada.
    ///
    /// O dono leu duas vezes que "a média não dá força": a FOR dela não é
    /// atributo do template, entra em `effective_stats` como ponto alocado, e
    /// por isso não aparecia em nenhuma lista de atributos. A ficha da bolsa
    /// foi corrigida; esta tela ficou, e ele voltou a ler o mesmo.
    #[test]
    fn o_craft_diz_que_a_armadura_media_da_forca() {
        let r = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|r| r.output_item_id == shared::item_id::ARMADURA_MEDIA)
            .expect("não há receita de armadura média");
        let f = shared::items::faixas_do_roll(r.output_item_id, r.output_item_level);
        for nivel in [1u32, 30, 60] {
            let (da, _) = o_que_da(&r, &f, "Medium Armour", nivel);
            let esperado = shared::for_da_armadura(shared::item_id::ARMADURA_MEDIA, nivel);
            assert!(esperado > 0, "medium armour stopped giving STR");
            assert!(
                da.contains(&format!("Strength +{esperado}")),
                "nível {nivel}: '{da}' não diz a Força que a peça empresta"
            );
        }
        // E a peça que NÃO empresta FOR não ganha a linha.
        let leve = shared::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|r| r.output_item_id == shared::item_id::ARMADURA_LEVE);
        if let Some(l) = leve {
            let fl = shared::items::faixas_do_roll(l.output_item_id, l.output_item_level);
            let (da, _) = o_que_da(&l, &fl, "Light Armour", 30);
            assert!(!da.contains("Strength"), "a leve não empresta FOR: {da}");
        }
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
        // Contado das tabelas, nao escrito: a aba tem uma linha por peca POR
        // COR, e em 28/09/2026 entrou a quinta cor (Lendaria). Um 16 na mao
        // reprovava aqui sem nada estar errado na aba.
        let por_cor = shared::receitas::PECAS
            .iter()
            .filter(|p| p.2 == categoria::ARMA)
            .count();
        assert_eq!(
            armas.len(),
            por_cor * shared::receitas::FAIXAS.len(),
            "{por_cor} armas x {} cores",
            shared::receitas::FAIXAS.len()
        );
        assert!(armas.iter().all(|r| r.category == categoria::ARMA));
        assert!(armas.windows(2).all(|w| w[0].nivel_min <= w[1].nivel_min));
        c.aba = 3;
        assert!(c
            .da_aba()
            .iter()
            .all(|r| r.category == categoria::ACESSORIO));
    }
}
