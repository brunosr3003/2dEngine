//! "Onde obter" (docs/ONDE_OBTER.md): a lupa ao lado de todo material pedido
//! (Craft, Forja, Bolsa, Mercado, barra de itens, missao e diaria de juntar
//! item) abre a lista de onde aquele item sai — e um "Ir" que leva o
//! personagem ate' la'.
//!
//! As fontes vem do servidor (`ResourceSources`), montadas das tabelas de loot,
//! coleta, lojas, receitas e missoes. Aqui so' se resolve ONDE na ilha atual
//! (zona do bicho, regiao do recurso, chefe, vendedor) e o que o "Ir" faz:
//! - coleta: vai ate' a regiao e liga o AUTO COLETA so' daquele tipo;
//! - bicho: vai ate' a zona e liga o AUTO COMBATE;
//! - chefe: vai ate' ele; muito acima do nivel, so' chega (nada liga sozinho);
//! - vendedor: vai ate' o NPC e fala com ele;
//! - craft: abre a receita; mercado: abre o Mercado buscando o item.
//!
//! Bicho ou chefe de OUTRA ilha mostra o nome dela, sem "Ir": viajar entre
//! ilhas so' existe pela historia, no Capitao do Porto.
//!
//! Nada abre por tecla: so' a lupa e o toque.
use std::collections::HashMap;

use macroquad::prelude::*;
use shared::protocol::{FonteDeItem, ItemResourceSources};

use crate::hud_estilo as estilo;
use crate::ir_para::{Alvo, Objetivo};
use crate::mapa::{chance_na_zona, nome_do_tipo, InfoDaIlha};

/// Zona ou chefe com nivel acima disto do jogador: avisa.
pub const ACIMA_DO_NIVEL: u32 = 5;
/// Chance (em %) pra zona contar como "onde o bicho nasce" (a do mapa).
const CHANCE_MINIMA_PCT: u8 = 15;
const VERMELHO: Color = Color::new(0.92, 0.42, 0.36, 1.0);
/// Sem "Ir" porque a fonte e' de outra ilha.
pub const OUTRA_ILHA: &str = "Outra ilha";

/// O que o "Ir" de uma fonte faz.
#[derive(Debug, Clone, PartialEq)]
pub enum Ir {
    Alvo(Alvo),
    AbrirCraft(u16),
    AbrirMercado(u16),
    /// Menu → Aventura → Dungeons.
    AbrirDungeons,
    /// O calendario de presenca.
    AbrirCalendario,
}

/// Uma linha do popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Opcao {
    pub titulo: String,
    pub detalhe: String,
    /// Em vermelho: nivel alto, chefe renascendo.
    pub aviso: Option<String>,
    pub ir: Option<Ir>,
    /// Sem "Ir": o porque ("Não há nesta ilha", "Outra ilha", "Em breve").
    pub sem_ir: Option<String>,
    /// (grupo, distancia): menor primeiro.
    pub ordem: (u8, u32),
}

/// O que o popup le do jogo.
pub struct Onde<'a> {
    pub info: Option<&'a InfoDaIlha>,
    /// (loja, nome do NPC, posicao) da vila (`Mapa::lojas_com_id`).
    pub lojas: &'a [(u32, String, Vec2)],
    pub eu: Option<Vec2>,
    pub nivel: u32,
    /// Item vinculado nao vai ao mercado.
    pub vinculado: bool,
    /// Indice da ilha atual em `terreno::ARQUIPELAGO` (`None` fora de ilha).
    pub ilha_atual: Option<u8>,
}

/// Indice da ilha de `zona` em `ARQUIPELAGO`.
pub fn ilha_da_zona(zona: &str) -> Option<u8> {
    shared::terreno::ARQUIPELAGO
        .iter()
        .position(|d| d.zona == zona)
        .map(|i| i as u8)
}

/// "Geleira, Ermo" — os nomes das ilhas, na ordem do arquipelago.
pub fn nomes_das_ilhas(ilhas: &[u8]) -> String {
    let mut v: Vec<u8> = ilhas.to_vec();
    v.sort_unstable();
    v.dedup();
    v.iter()
        .filter_map(|i| {
            shared::terreno::ARQUIPELAGO
                .get(*i as usize)
                .map(|d| d.nome)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn pct(chance: f32) -> String {
    let p = chance * 100.0;
    if p >= 10.0 || p == p.round() {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%").replace('.', ",")
    }
}

fn qtd(a: u32, b: u32) -> String {
    if a == b {
        format!("{a}")
    } else {
        format!("{a}–{b}")
    }
}

fn distancia(eu: Option<Vec2>, p: Vec2) -> u32 {
    eu.map_or(0, |e| e.distance(p).round() as u32)
}

/// Sem "Ir" pra bicho/chefe que nao se achou na ilha atual: de outra ilha
/// (diz qual, no detalhe), fora de ilha, ou nao ha' aqui.
fn sem_lugar(o: &mut Opcao, ilhas: &[u8], c: &Onde) {
    let fora_daqui = !ilhas.is_empty() && c.ilha_atual.is_none_or(|a| !ilhas.contains(&a));
    if !ilhas.is_empty() {
        let rotulo = if ilhas.len() == 1 { "Ilha" } else { "Ilhas" };
        o.detalhe = format!("{} · {rotulo}: {}", o.detalhe, nomes_das_ilhas(ilhas));
    }
    o.sem_ir = Some(if c.info.is_none() {
        "Fora de ilha".into()
    } else if fora_daqui {
        OUTRA_ILHA.into()
    } else {
        "Não há nesta ilha".into()
    });
}

/// As opcoes do item, na ordem da tela: o que da' pra ir e fazer ja' (coleta
/// e bicho do nivel, do mais perto), depois chefe e vendedor, craft, mercado,
/// missao e o que ainda nao existe.
pub fn opcoes(item: u16, fontes: &[FonteDeItem], c: &Onde) -> Vec<Opcao> {
    let mut v: Vec<Opcao> = fontes.iter().map(|f| opcao(f, c)).collect();
    if !c.vinculado {
        v.push(Opcao {
            titulo: "Mercado".into(),
            detalhe: "Comprar de outros jogadores".into(),
            aviso: None,
            ir: Some(Ir::AbrirMercado(item)),
            sem_ir: None,
            ordem: (6, 0),
        });
    }
    v.sort_by(|a, b| a.ordem.cmp(&b.ordem).then(a.titulo.cmp(&b.titulo)));
    v
}

fn opcao(f: &FonteDeItem, c: &Onde) -> Opcao {
    let fora = c.info.is_none();
    let mut o = Opcao {
        titulo: String::new(),
        detalhe: String::new(),
        aviso: None,
        ir: None,
        sem_ir: None,
        ordem: (5, 0),
    };
    match f {
        FonteDeItem::Coleta {
            tipo,
            chance,
            qty_min,
            qty_max,
        } => {
            o.titulo = format!("Coletar: {}", nome_do_tipo(*tipo));
            o.detalhe = format!("{} por coleta · {}", qtd(*qty_min, *qty_max), pct(*chance));
            let regiao = c.info.and_then(|i| {
                i.recursos
                    .iter()
                    .filter(|g| g.tipo == *tipo)
                    .min_by_key(|g| distancia(c.eu, vec2(g.centro[0], g.centro[1])))
            });
            match regiao {
                Some(g) => {
                    let pos = vec2(g.centro[0], g.centro[1]);
                    o.ordem = (0, distancia(c.eu, pos));
                    o.ir = Some(Ir::Alvo(Alvo {
                        objetivo: Objetivo::Coleta(*tipo),
                        pos,
                        raio: g.raio,
                        rotulo: nome_do_tipo(*tipo).to_string(),
                    }));
                }
                None => {
                    // Toda ilha tem arvore e as quatro cores de pedra (a cor
                    // sai da altura relativa ao pico): so' nao ha' perto.
                    o.ordem = (5, 0);
                    o.sem_ir = Some(
                        if fora {
                            "Fora de ilha"
                        } else {
                            "Não há nesta ilha"
                        }
                        .into(),
                    );
                }
            }
        }
        FonteDeItem::Mob {
            kind,
            nome,
            chance,
            qty_min,
            qty_max,
            ilhas,
        } => {
            o.titulo = format!("Caçar: {nome}");
            let zona = c.info.and_then(|i| {
                let com: Vec<_> = i
                    .zonas
                    .iter()
                    .filter(|z| chance_na_zona(z, *kind) > 0)
                    .collect();
                let boas: Vec<_> = com
                    .iter()
                    .copied()
                    .filter(|z| chance_na_zona(z, *kind) >= CHANCE_MINIMA_PCT)
                    .collect();
                let lista = if boas.is_empty() { com } else { boas };
                lista
                    .into_iter()
                    .min_by_key(|z| distancia(c.eu, vec2(z.centro[0], z.centro[1])))
            });
            match zona {
                Some(z) => {
                    let pos = vec2(z.centro[0], z.centro[1]);
                    o.detalhe = format!(
                        "Nv {}–{} · {} por morte · {}",
                        z.lv_min,
                        z.lv_max,
                        pct(*chance),
                        qtd(*qty_min, *qty_max)
                    );
                    let alto = z.lv_min as u32 > c.nivel + ACIMA_DO_NIVEL;
                    if alto {
                        o.aviso = Some(format!("Zona nível {}+: acima do seu nível", z.lv_min));
                    }
                    o.ordem = (if alto { 3 } else { 0 }, distancia(c.eu, pos));
                    o.ir = Some(Ir::Alvo(Alvo {
                        objetivo: Objetivo::Combate,
                        pos,
                        raio: z.raio,
                        rotulo: nome.clone(),
                    }));
                }
                None => {
                    o.detalhe = format!("{} por morte · {}", pct(*chance), qtd(*qty_min, *qty_max));
                    sem_lugar(&mut o, ilhas, c);
                }
            }
        }
        FonteDeItem::ChefeDoMundo {
            kind,
            nome,
            nivel,
            chance,
            ilha,
        } => {
            o.titulo = format!("Chefe: {nome}");
            o.detalhe = format!("Chefe Nv {nivel} · {}", pct(*chance));
            match c
                .info
                .and_then(|i| i.chefes.iter().find(|ch| ch.kind == *kind))
            {
                Some(ch) => {
                    let pos = vec2(ch.centro[0], ch.centro[1]);
                    let alto = *nivel as u32 > c.nivel + ACIMA_DO_NIVEL;
                    if alto {
                        o.aviso = Some(format!(
                            "Nível {nivel}: bem acima do seu — o combate não liga sozinho"
                        ));
                    } else if !ch.vivo {
                        o.aviso = Some("Renascendo".into());
                    }
                    o.ordem = (if alto { 4 } else { 1 }, distancia(c.eu, pos));
                    let objetivo = if alto {
                        Objetivo::Lugar
                    } else {
                        Objetivo::Combate
                    };
                    o.ir = Some(Ir::Alvo(Alvo {
                        objetivo,
                        pos,
                        raio: 10.0,
                        rotulo: nome.clone(),
                    }));
                }
                None => sem_lugar(&mut o, &[*ilha], c),
            }
        }
        FonteDeItem::Vendedor { loja, nome, preco } => {
            let npc = c
                .lojas
                .iter()
                .filter(|l| l.0 == *loja)
                .min_by_key(|l| distancia(c.eu, l.2));
            o.titulo = format!("Comprar: {}", npc.map_or(nome.as_str(), |n| n.1.as_str()));
            o.detalhe = format!("{preco} de ouro");
            match npc {
                Some((_, n, pos)) => {
                    o.ordem = (1, distancia(c.eu, *pos));
                    o.ir = Some(Ir::Alvo(Alvo {
                        objetivo: Objetivo::Npc,
                        pos: *pos,
                        raio: 0.0,
                        rotulo: n.clone(),
                    }));
                }
                // A vila de toda ilha tem o mesmo Alquimista.
                None => {
                    o.sem_ir = Some(
                        if fora {
                            "Fora de ilha"
                        } else {
                            "Não há nesta ilha"
                        }
                        .into(),
                    )
                }
            }
        }
        FonteDeItem::Craft {
            receita,
            nome,
            nivel_min,
        } => {
            o.titulo = format!("Criar: {nome}");
            o.detalhe = format!("Craft · nível mínimo {}", (*nivel_min).max(1));
            if c.nivel < *nivel_min as u32 {
                o.aviso = Some(format!("Requer nível {nivel_min}"));
            }
            o.ordem = (2, *nivel_min as u32);
            o.ir = Some(Ir::AbrirCraft(*receita));
        }
        FonteDeItem::Missao { titulo, diaria, .. } => {
            o.titulo = format!("Recompensa: {titulo}");
            o.detalhe = if *diaria {
                "Missão diária".into()
            } else {
                "Missão".into()
            };
            o.ordem = (7, 0);
            o.sem_ir = Some("Missão".into());
        }
        FonteDeItem::DungeonRaid => {
            o.titulo = "Chefe de dungeon (Gruta)".into();
            o.detalhe = "15/10/6/3/1% por faixa · Caçada em breve".into();
            o.ordem = (8, 0);
            o.ir = Some(Ir::AbrirDungeons);
        }
        FonteDeItem::Calendario { dias } => {
            o.titulo = "Calendário de presença".into();
            let lista: Vec<String> = dias.iter().map(|d| d.to_string()).collect();
            o.detalhe = format!(
                "Resgate diário · dia{} {}",
                if dias.len() > 1 { "s" } else { "" },
                lista.join(", ")
            );
            o.ordem = (5, 0);
            o.ir = Some(Ir::AbrirCalendario);
        }
    }
    o
}

#[derive(Default)]
pub struct OndeObter {
    fontes: HashMap<u16, Vec<FonteDeItem>>,
    /// Item do popup aberto.
    pub item: Option<u16>,
    rolagem: f32,
    arrasto: Option<f32>,
}

impl OndeObter {
    pub fn define(&mut self, itens: Vec<ItemResourceSources>) {
        self.fontes = itens.into_iter().map(|e| (e.item_id, e.sources)).collect();
    }

    pub fn fontes_de(&self, item: u16) -> &[FonteDeItem] {
        self.fontes.get(&item).map_or(&[], |v| v.as_slice())
    }

    pub fn abrir(&mut self, item: u16) {
        self.item = Some(item);
        self.rolagem = 0.0;
        self.arrasto = None;
    }

    pub fn fechar(&mut self) {
        self.item = None;
    }

    pub fn aberto(&self) -> bool {
        self.item.is_some()
    }

    fn painel() -> Rect {
        let f = estilo::fator_texto();
        let s = crate::hud_layout::tela_segura();
        let (w, h) = ((600.0 * f).min(s.w - 16.0), (560.0 * f).min(s.h - 16.0));
        Rect::new(s.center().x - w * 0.5, s.center().y - h * 0.5, w, h)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto() && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha o popup. `Some` no quadro em que um "Ir" foi tocado.
    pub fn desenha(&mut self, nome: &str, ops: &[Opcao]) -> Option<Ir> {
        let item = self.item?;
        crate::hud_layout::escurece(0.55);
        let f = estilo::fator_texto();
        let p = Self::painel();
        estilo::painel(p);
        crate::bolsa::icone_do_item(
            Rect::new(p.x + 14.0 * f, p.y + 10.0 * f, 40.0 * f, 40.0 * f),
            item,
            1.0,
        );
        estilo::texto_ajustado(
            &format!("Onde obter: {nome}"),
            p.x + 62.0 * f,
            p.y + 36.0 * f,
            p.w - 130.0 * f,
            20,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - 50.0 * f, p.y + 8.0 * f, 42.0 * f, 38.0 * f);
        if crate::ui::botao(fechar, "x", true) {
            self.fechar();
            return None;
        }
        let lista = Rect::new(
            p.x + 10.0 * f,
            p.y + 58.0 * f,
            p.w - 20.0 * f,
            p.h - 68.0 * f,
        );
        let linha = 70.0 * f;
        let mouse = Vec2::from(mouse_position());
        let total = ops.len() as f32 * linha;
        let max = (total - lista.h).max(0.0);
        if lista.contains(mouse) {
            let (_, roda) = mouse_wheel();
            self.rolagem -= roda.signum() * linha;
        }
        // Arrasto de dedo rola a lista.
        if is_mouse_button_pressed(MouseButton::Left) && lista.contains(mouse) {
            self.arrasto = Some(mouse.y);
        }
        let mut arrastou = false;
        if let Some(y0) = self.arrasto {
            if is_mouse_button_down(MouseButton::Left) {
                let dy = mouse.y - y0;
                if dy.abs() > 8.0 {
                    self.rolagem -= dy;
                    self.arrasto = Some(mouse.y);
                    arrastou = true;
                }
            } else {
                self.arrasto = None;
            }
        }
        self.rolagem = self.rolagem.clamp(0.0, max);
        if ops.is_empty() {
            estilo::texto(
                lista.x + 8.0,
                lista.y + 26.0 * f,
                "Nenhuma fonte conhecida para este item.",
                15,
                estilo::SUAVE,
            );
        }
        let mut saida = None;
        for (i, o) in ops.iter().enumerate() {
            let r = Rect::new(
                lista.x,
                lista.y + i as f32 * linha - self.rolagem,
                lista.w,
                linha - 6.0 * f,
            );
            if r.y + r.h < lista.y || r.y > lista.y + lista.h {
                continue;
            }
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.04));
            let texto_w = r.w - 130.0 * f;
            estilo::texto_ajustado(
                &o.titulo,
                r.x + 12.0 * f,
                r.y + 22.0 * f,
                texto_w,
                16,
                estilo::TEXTO,
            );
            estilo::texto_ajustado(
                &o.detalhe,
                r.x + 12.0 * f,
                r.y + 42.0 * f,
                texto_w,
                13,
                estilo::SUAVE,
            );
            if let Some(a) = &o.aviso {
                estilo::texto_ajustado(a, r.x + 12.0 * f, r.y + 59.0 * f, texto_w, 12, VERMELHO);
            }
            let b = Rect::new(
                r.x + r.w - 110.0 * f,
                r.y + (r.h - 40.0 * f) * 0.5,
                100.0 * f,
                40.0 * f,
            );
            match (&o.ir, &o.sem_ir) {
                (Some(ir), _) => {
                    let visivel = b.y >= lista.y && b.y + b.h <= lista.y + lista.h;
                    let rotulo = match ir {
                        Ir::Alvo(_) => "Ir",
                        Ir::AbrirCraft(_) => "Abrir",
                        Ir::AbrirMercado(_) => "Buscar",
                        Ir::AbrirDungeons | Ir::AbrirCalendario => "Abrir",
                    };
                    if crate::ui::botao(b, rotulo, visivel) && visivel && !arrastou {
                        saida = Some(ir.clone());
                    }
                }
                (None, Some(motivo)) => {
                    let w = estilo::medir(motivo, 13);
                    if motivo == "Em breve" {
                        crate::menu_missoes::cadeado(
                            vec2(b.x + b.w - w - 20.0 * f, b.center().y),
                            7.0 * f,
                            estilo::SUAVE,
                        );
                    }
                    estilo::texto(
                        b.x + b.w - w,
                        b.center().y + 5.0 * f,
                        motivo,
                        13,
                        estilo::SUAVE,
                    );
                }
                (None, None) => {}
            }
        }
        if saida.is_some() {
            self.fechar();
        }
        saida
    }
}

/// A lupa "Onde obter". `true` no toque. Pequena no desenho, com a caixa de
/// toque que o `r` pedir (deixe >= 30 px).
pub fn botao(r: Rect) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = r.contains(m);
    estilo::cartao(r, sobre, false);
    let cor = if sobre { estilo::ACENTO } else { estilo::OURO };
    desenha_lupa(r, cor);
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

/// A lupa desenhada DENTRO de um icone de item (canto de cima, a' direita): o
/// toque em qualquer ponto do icone vale — no celular um alvo de 20 px nao se
/// acerta. `true` no toque.
pub fn lupa_no_icone(icone: Rect) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = icone.contains(m);
    let s = (icone.w * 0.42).max(16.0);
    let canto = Rect::new(icone.x + icone.w - s, icone.y, s, s);
    estilo::ret_arredondado(canto, s * 0.25, Color::new(0.0, 0.0, 0.0, 0.55));
    desenha_lupa(canto, if sobre { estilo::ACENTO } else { estilo::OURO });
    sobre && is_mouse_button_pressed(MouseButton::Left)
}

fn desenha_lupa(r: Rect, cor: Color) {
    let s = r.w.min(r.h);
    let c = r.center() + vec2(-s * 0.08, -s * 0.08);
    let raio = s * 0.22;
    draw_circle_lines(c.x, c.y, raio, (s * 0.07).max(1.5), cor);
    let d = vec2(0.707, 0.707);
    estilo::traco(
        c + d * raio,
        c + d * (raio + s * 0.24),
        (s * 0.09).max(2.0),
        cor,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::protocol::{RegiaoNoMapa, ZonaNoMapa};

    fn info() -> InfoDaIlha {
        InfoDaIlha {
            zonas: vec![
                ZonaNoMapa {
                    centro: [100.0, 0.0],
                    raio: 40.0,
                    lv_min: 1,
                    lv_max: 3,
                    bichos: vec![(0, 80), (1, 20)],
                },
                ZonaNoMapa {
                    centro: [400.0, 0.0],
                    raio: 40.0,
                    lv_min: 30,
                    lv_max: 32,
                    bichos: vec![(5, 100)],
                },
            ],
            recursos: vec![
                RegiaoNoMapa {
                    centro: [300.0, 0.0],
                    raio: 10.0,
                    tipo: 2,
                    contagem: 5,
                },
                RegiaoNoMapa {
                    centro: [50.0, 0.0],
                    raio: 10.0,
                    tipo: 2,
                    contagem: 3,
                },
            ],
            nomes: HashMap::new(),
            rendimentos: HashMap::new(),
            chefes: vec![shared::bosses::ChefeNoMapa {
                kind: 18,
                nome: "Arquimago".into(),
                nivel: 60,
                centro: [900.0, 0.0],
                vivo: true,
            }],
        }
    }

    fn onde<'a>(i: &'a InfoDaIlha, lojas: &'a [(u32, String, Vec2)], vinculado: bool) -> Onde<'a> {
        Onde {
            info: Some(i),
            lojas,
            eu: Some(Vec2::ZERO),
            nivel: 5,
            vinculado,
            ilha_atual: Some(0),
        }
    }

    fn alvo(o: &Opcao) -> &Alvo {
        match &o.ir {
            Some(Ir::Alvo(a)) => a,
            outro => panic!("sem alvo: {outro:?}"),
        }
    }

    fn mob(kind: u16, nome: &str, chance: f32, ilhas: Vec<u8>) -> FonteDeItem {
        FonteDeItem::Mob {
            kind,
            nome: nome.into(),
            chance,
            qty_min: 1,
            qty_max: 1,
            ilhas,
        }
    }

    #[test]
    fn coleta_vai_a_regiao_mais_perto_do_tipo_e_liga_so_ele() {
        let i = info();
        let v = opcoes(
            1,
            &[FonteDeItem::Coleta {
                tipo: 2,
                chance: 0.2,
                qty_min: 3,
                qty_max: 6,
            }],
            &onde(&i, &[], false),
        );
        let a = alvo(&v[0]);
        assert_eq!((a.objetivo, a.pos), (Objetivo::Coleta(2), vec2(50.0, 0.0)));
        let sem = opcoes(
            1,
            &[FonteDeItem::Coleta {
                tipo: 4,
                chance: 0.2,
                qty_min: 1,
                qty_max: 1,
            }],
            &onde(&i, &[], true),
        );
        assert_eq!(
            (sem[0].ir.clone(), sem[0].sem_ir.as_deref()),
            (None, Some("Não há nesta ilha"))
        );
        assert_eq!(sem.len(), 1, "vinculado: sem Mercado");
    }

    #[test]
    fn bicho_liga_combate_e_avisa_nivel_alto() {
        let i = info();
        let v = opcoes(
            1,
            &[mob(5, "Owlbear", 0.25, vec![0, 1])],
            &onde(&i, &[], true),
        );
        assert_eq!(alvo(&v[0]).objetivo, Objetivo::Combate);
        assert!(v[0].aviso.is_some(), "zona 30+ pro nivel 5");
        let lobo = opcoes(1, &[mob(0, "Lobo", 0.08, vec![0])], &onde(&i, &[], true));
        assert!(lobo[0].aviso.is_none());
        assert_eq!(alvo(&lobo[0]).pos, vec2(100.0, 0.0));
    }

    #[test]
    fn bicho_de_outra_ilha_diz_qual_e_nao_tem_ir() {
        let i = info();
        let v = opcoes(1, &[mob(9, "Mago", 0.2, vec![2, 3])], &onde(&i, &[], true));
        assert_eq!(
            (v[0].ir.clone(), v[0].sem_ir.as_deref()),
            (None, Some(OUTRA_ILHA))
        );
        assert!(
            v[0].detalhe.ends_with("Ilhas: Ermo, Planalto"),
            "{}",
            v[0].detalhe
        );
        // Nasce aqui, mas nenhuma zona perto no mapa.
        let aqui = opcoes(1, &[mob(9, "Mago", 0.2, vec![0])], &onde(&i, &[], true));
        assert_eq!(aqui[0].sem_ir.as_deref(), Some("Não há nesta ilha"));
    }

    #[test]
    fn chefe_de_outra_ilha_diz_a_ilha() {
        let i = info();
        let v = opcoes(
            1,
            &[FonteDeItem::ChefeDoMundo {
                kind: 13,
                nome: "Tigre das Neves".into(),
                nivel: 24,
                chance: 0.0075,
                ilha: 1,
            }],
            &onde(&i, &[], true),
        );
        assert_eq!(v[0].sem_ir.as_deref(), Some(OUTRA_ILHA));
        assert!(v[0].detalhe.ends_with("Ilha: Geleira"), "{}", v[0].detalhe);
    }

    #[test]
    fn chefe_muito_acima_so_chega() {
        let i = info();
        let chefe = FonteDeItem::ChefeDoMundo {
            kind: 18,
            nome: "Arquimago".into(),
            nivel: 60,
            chance: 0.0025,
            ilha: 3,
        };
        let v = opcoes(1, std::slice::from_ref(&chefe), &onde(&i, &[], true));
        assert_eq!(alvo(&v[0]).objetivo, Objetivo::Lugar);
        assert!(v[0].aviso.is_some());
        let mut perto = onde(&i, &[], true);
        perto.nivel = 58;
        let v = opcoes(1, &[chefe], &perto);
        assert_eq!(alvo(&v[0]).objetivo, Objetivo::Combate);
    }

    #[test]
    fn vendedor_craft_mercado_missao_e_dungeon() {
        let i = info();
        let lojas = vec![(3u32, "Alquimista Ana".to_string(), vec2(10.0, 10.0))];
        let fontes = [
            FonteDeItem::DungeonRaid,
            FonteDeItem::Missao {
                quest: 7,
                titulo: "Pedreira".into(),
                diaria: false,
            },
            FonteDeItem::Craft {
                receita: 1000,
                nome: "Katana".into(),
                nivel_min: 1,
            },
            FonteDeItem::Vendedor {
                loja: 3,
                nome: "Poções".into(),
                preco: 10,
            },
            FonteDeItem::Coleta {
                tipo: 2,
                chance: 0.5,
                qty_min: 1,
                qty_max: 1,
            },
        ];
        let v = opcoes(2, &fontes, &onde(&i, &lojas, false));
        let titulos: Vec<&str> = v.iter().map(|o| o.titulo.as_str()).collect();
        assert_eq!(
            titulos,
            vec![
                "Coletar: Pedra verde",
                "Comprar: Alquimista Ana",
                "Criar: Katana",
                "Mercado",
                "Recompensa: Pedreira",
                "Chefe de dungeon (Gruta)"
            ]
        );
        assert_eq!(alvo(&v[1]).objetivo, Objetivo::Npc);
        assert_eq!(v[2].ir, Some(Ir::AbrirCraft(1000)));
        assert_eq!(v[3].ir, Some(Ir::AbrirMercado(2)));
        assert_eq!(
            v[5].ir,
            Some(Ir::AbrirDungeons),
            "a chave abre a janela das dungeons"
        );
    }

    #[test]
    fn fora_de_ilha_nao_tem_ir_de_lugar() {
        let c = Onde {
            info: None,
            lojas: &[],
            eu: None,
            nivel: 1,
            vinculado: true,
            ilha_atual: None,
        };
        let v = opcoes(1, &[mob(0, "Lobo", 0.1, vec![0])], &c);
        assert_eq!(
            (v[0].ir.clone(), v[0].sem_ir.as_deref()),
            (None, Some("Fora de ilha"))
        );
        assert!(v[0].detalhe.ends_with("Ilha: Bosque"));
    }

    #[test]
    fn nomes_e_indice_das_ilhas() {
        assert_eq!(nomes_das_ilhas(&[3, 1, 1]), "Geleira, Planalto");
        assert_eq!(ilha_da_zona("ilha_deserto"), Some(2));
        assert_eq!(ilha_da_zona("dungeon"), None);
    }
}
