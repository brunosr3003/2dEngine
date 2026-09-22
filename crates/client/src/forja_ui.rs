//! Painel da FORJA: escolhe uma peca (vestida ou da bolsa), mostra o nivel,
//! a chance do proximo, o custo e o risco, e refina.
//!
//! Regras de `shared::forja` (docs/ITENS.md): +1..+12, seguro ate' +5, do +6
//! em diante falhar DESTROI a peca. Quem sorteia e cobra e' o servidor
//! (`Refinar` → `RefinoResultado`); aqui so' se mostra.
//!
//! Abre pelo HUD ou clicando no Ferreiro da vila — nunca por tecla.
//! API pro HUD: `abrir()`, `fechar()`, `aberto()`, `pega_mouse()`,
//! `resultado(..)` e `desenha(..) -> Option<ClientMessage>`.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::forja::{self, resultado, Grau};
use shared::protocol::{AlvoDaForja, ClientMessage};
use shared::{item_id, EquipSlot, Equipment, InventorySlot, ItemInstance};

use crate::hud_estilo::{self as estilo, u};
use crate::vox::VoxCache;
use macroquad::material::Material;

const LARGURA: f32 = 720.0;
const ALTURA: f32 = 500.0;
const CELULA: f32 = 58.0;
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);
const VERMELHO: Color = Color::new(0.90, 0.40, 0.34, 1.0);
const AMARELO: Color = Color::new(0.95, 0.78, 0.30, 1.0);

/// O que o painel mostra de uma peca.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Info {
    pub nivel: u8,
    pub no_topo: bool,
    /// Chance do proximo nivel, em %.
    pub chance: u8,
    pub darksteel: u32,
    pub cobre: u32,
    /// Falhar agora destroi a peca.
    pub risco: bool,
}

pub fn info(inst: &ItemInstance) -> Info {
    let nivel = inst.refinement;
    let grau = Grau::de_u8(inst.rarity.clamp(1, 5)).unwrap_or(Grau::Comum);
    let (darksteel, cobre) = forja::custo_em_jogo(grau);
    let no_topo = nivel >= forja::REFINO_MAX;
    Info {
        nivel,
        no_topo,
        chance: if no_topo {
            0
        } else {
            forja::chance_de_refino(nivel + 1)
        },
        darksteel,
        cobre,
        risco: !no_topo && nivel + 1 > forja::REFINO_SEGURO,
    }
}

/// "Refinar mesmo assim?" — a pergunta antes de arriscar a peça.
///
/// `None` = ainda esperando; `Some(true)` = manda ver; `Some(false)` = não.
///
/// Existe porque o aviso ESCRITO não bastava. O painel já dizia "se falhar, a
/// peça é DESTRUÍDA" numa linha vermelha ao lado do botão, e o botão já se
/// chamava "Refinar (arriscado)" — mas quem lê a mesma linha vinte vezes
/// para de lê-la, e o toque que custa uma peça épica acaba sendo o mesmo
/// toque de sempre. Um gesto a mais é o que separa a decisão do reflexo.
///
/// A chance aparece NOS DOIS SENTIDOS: "30% de subir" e "70% de destruir" são
/// o mesmo número, e é o segundo que pesa na hora de decidir.
fn confirma_refino(nome: &str, i: &Info) -> Option<bool> {
    let f = estilo::fator_texto();
    let seguro = crate::hud_layout::tela_segura();
    let (w, h) = (
        (540.0 * f).min(seguro.w - 24.0),
        (270.0 * f).min(seguro.h - 24.0),
    );
    let r = Rect::new(
        seguro.center().x - w * 0.5,
        seguro.center().y - h * 0.5,
        w,
        h,
    );
    crate::hud_layout::escurece(0.55);
    estilo::painel_destaque(r, VERMELHO);
    let x = r.x + 24.0 * f;
    estilo::texto_forte(x, r.y + 42.0 * f, "Refinar mesmo assim?", 20, VERMELHO);
    estilo::texto_ajustado(
        &format!("{nome} +{} → +{}", i.nivel, i.nivel + 1),
        x,
        r.y + 80.0 * f,
        w - 48.0 * f,
        17,
        estilo::TEXTO,
    );
    estilo::texto_ajustado(
        &format!(
            "{}% de subir · {}% de DESTRUIR a peça.",
            i.chance,
            100u8.saturating_sub(i.chance)
        ),
        x,
        r.y + 110.0 * f,
        w - 48.0 * f,
        16,
        VERMELHO,
    );
    estilo::texto_ajustado(
        "Destruída, ela não volta: o refino e os materiais dela vão junto.",
        x,
        r.y + 140.0 * f,
        w - 48.0 * f,
        14,
        estilo::SUAVE,
    );
    let bw = (w - 60.0 * f) * 0.5;
    let bh = 46.0 * f;
    let by = r.y + r.h - 24.0 * f - bh;
    let cancelar = Rect::new(x, by, bw, bh);
    let sim = Rect::new(cancelar.x + bw + 12.0 * f, by, bw, bh);
    let m = Vec2::from(mouse_position());
    // CANCELAR é o botão de destaque, e o "sim" é o apagado: aqui o caminho
    // seguro é o que deve estar debaixo do polegar.
    estilo::cartao(cancelar, cancelar.contains(m), true);
    estilo::texto_centro_forte(
        cancelar.center().x,
        cancelar.center().y + 6.0 * f,
        "Cancelar",
        16,
        estilo::OURO,
    );
    estilo::cartao(sim, sim.contains(m), false);
    estilo::texto_centro(
        sim.center().x,
        sim.center().y + 6.0 * f,
        "Sim, refinar",
        16,
        VERMELHO,
    );
    if !crate::foco::clique() {
        return None;
    }
    if sim.contains(m) {
        return Some(true);
    }
    // Tocar FORA cancela, como em toda janela do jogo — e aqui isso é a
    // resposta certa por acidente, que é como tem que ser.
    if cancelar.contains(m) || !r.contains(m) {
        return Some(false);
    }
    None
}

/// A frase do resultado.
pub fn texto_do_resultado(res: u8, nivel: u8, nome: &str, motivo: &str) -> (String, Color) {
    match res {
        resultado::SUBIU => (format!("Sucesso! {nome} agora é +{nivel}."), VERDE),
        resultado::FALHOU => (
            format!("Falhou. {nome} continua +{nivel} (só o material foi)."),
            AMARELO,
        ),
        resultado::DESTRUIU => (format!("Falhou e {nome} foi destruída."), VERMELHO),
        _ => (
            if motivo.is_empty() {
                "Não deu pra refinar.".to_string()
            } else {
                format!("Não refinou: {motivo}.")
            },
            VERMELHO,
        ),
    }
}

fn tem(slots: &[InventorySlot], id: u16) -> u32 {
    slots
        .iter()
        .filter(|s| s.item_id == id && s.instance.is_none() && s.qty > 0)
        .map(|s| s.qty)
        .sum()
}

/// As pecas refinaveis: as vestidas primeiro, depois as da bolsa.
pub fn pecas(slots: &[InventorySlot], equip: &Equipment) -> Vec<(AlvoDaForja, u16, ItemInstance)> {
    let mut v = Vec::new();
    for s in EquipSlot::TODOS {
        if let (Some(id), Some(inst)) = (equip.get(s), equip.get_inst(s)) {
            v.push((AlvoDaForja::Equipado(s), id, inst));
        }
    }
    for (i, s) in slots.iter().enumerate() {
        if let (true, Some(inst)) = (s.qty > 0, s.instance) {
            v.push((AlvoDaForja::Bolsa(i as u16), s.item_id, inst));
        }
    }
    v
}

#[derive(Default)]
pub struct Forja {
    aberto: bool,
    sel: Option<AlvoDaForja>,
    /// (resultado, texto, cor, quando) — pisca o painel.
    aviso: Option<(u8, String, Color, f64)>,
    /// Lupa tocada num material: o item pro "Onde obter".
    pub onde_obter: Option<u16>,
    /// Refino ARRISCADO esperando um "sim".
    ///
    /// Do +6 em diante falhar DESTRÓI a peça, e o painel só dizia isso numa
    /// linha vermelha ao lado do botão. Quem já tinha lido aquela linha vinte
    /// vezes parava de lê-la, e o toque que custava uma peça épica era o
    /// mesmo toque de sempre. O dono: "itens quando tiver chance de
    /// destruição no refino tem que ter um aviso com sim pra tocar antes de
    /// refinar".
    ///
    /// Guarda o ALVO, e não um booleano: entre abrir a pergunta e responder,
    /// a seleção do painel pode mudar, e confirmar tem que valer para a peça
    /// que foi perguntada.
    confirmar: Option<(AlvoDaForja, u16, ItemInstance)>,
}

impl Forja {
    pub fn abrir(&mut self) {
        self.aberto = true;
    }

    /// Abre com a peca `alvo` ja' escolhida (o "Refinar" do cartao da bolsa).
    pub fn abrir_em(&mut self, alvo: shared::protocol::AlvoDaForja) {
        self.sel = Some(alvo);
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

    /// `RefinoResultado` do servidor.
    pub fn resultado(&mut self, res: u8, nivel: u8, nome: &str, motivo: &str, agora: f64) {
        let (txt, cor) = texto_do_resultado(res, nivel, nome, motivo);
        if res == resultado::DESTRUIU {
            self.sel = None;
        }
        self.aviso = Some((res, txt, cor, agora));
    }

    pub fn desenha(
        &mut self,
        slots: &[InventorySlot],
        equip: &Equipment,
        nomes: &HashMap<u16, String>,
        agora: f64,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(slots, equip, nomes, agora, palco))
    }

    fn desenha_na_escala(
        &mut self,
        slots: &[InventorySlot],
        equip: &Equipment,
        nomes: &HashMap<u16, String>,
        agora: f64,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<ClientMessage> {
        let p = Self::painel();
        estilo::painel(p);
        // Pisca na cor do resultado.
        if let Some((_, _, cor, t)) = &self.aviso {
            let f = (1.0 - (agora - t) as f32 / 1.2).clamp(0.0, 1.0);
            if f > 0.0 {
                draw_rectangle_lines(
                    p.x - u(3.0),
                    p.y - u(3.0),
                    p.w + u(6.0),
                    p.h + u(6.0),
                    u(4.0),
                    Color::new(cor.r, cor.g, cor.b, f),
                );
            }
        }
        estilo::texto(p.x + u(18.0), p.y + u(32.0), "Forja", 22, estilo::OURO);
        estilo::texto(
            p.x + u(92.0),
            p.y + u(31.0),
            "até +5 é seguro · do +6 em diante falhar destrói a peça",
            13,
            estilo::SUAVE,
        );
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
        let lista = pecas(slots, equip);
        if let Some(sel) = self.sel {
            if !lista.iter().any(|(a, _, _)| *a == sel) {
                self.sel = None;
            }
        }
        if self.sel.is_none() {
            self.sel = lista.first().map(|(a, _, _)| *a);
        }
        // Grade de pecas.
        let grade = Rect::new(p.x + u(14.0), p.y + u(52.0), u(6.0) * CELULA, p.h - u(64.0));
        let mouse = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        if lista.is_empty() {
            estilo::texto(
                grade.x + u(4.0),
                grade.y + u(24.0),
                "Nenhuma peça refinável.",
                15,
                estilo::SUAVE,
            );
        }
        for (i, (alvo, id, inst)) in lista.iter().enumerate() {
            let (col, lin) = (i % 6, i / 6);
            let r = Rect::new(
                grade.x + col as f32 * CELULA,
                grade.y + lin as f32 * CELULA,
                CELULA - u(6.0),
                CELULA - u(6.0),
            );
            if r.y + r.h > grade.y + grade.h {
                break;
            }
            let marcada = self.sel == Some(*alvo);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                Color::new(1.0, 1.0, 1.0, if marcada { 0.16 } else { 0.05 }),
            );
            if marcada {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, u(2.0), estilo::OURO);
            }
            crate::bolsa::icone_do_item_com(r, *id, 1.0, palco);
            if inst.refinement > 0 {
                estilo::texto(
                    r.x + u(3.0),
                    r.y + u(14.0),
                    &format!("+{}", inst.refinement),
                    13,
                    estilo::OURO,
                );
            }
            if matches!(alvo, AlvoDaForja::Equipado(_)) {
                estilo::texto(r.x + r.w - u(12.0), r.y + r.h - u(4.0), "E", 12, estilo::AUTO);
            }
            if r.contains(mouse) && clicou {
                self.sel = Some(*alvo);
            }
        }
        // Detalhe.
        let d = Rect::new(
            grade.x + grade.w + u(12.0),
            p.y + u(52.0),
            p.x + p.w - grade.x - grade.w - u(26.0),
            p.h - u(64.0),
        );
        let mut pedido = None;
        if let Some((alvo, id, inst)) = lista.iter().find(|(a, _, _)| Some(*a) == self.sel) {
            let i = info(inst);
            crate::bolsa::icone_do_item_com(Rect::new(d.x, d.y, u(64.0), u(64.0)), *id, 1.0, palco);
            estilo::texto_ajustado(
                &nome(*id),
                d.x + u(74.0),
                d.y + u(24.0),
                d.w - u(80.0),
                18,
                estilo::TEXTO,
            );
            let grau = Grau::de_u8(inst.rarity.clamp(1, 5)).unwrap_or(Grau::Comum);
            let onde = if matches!(alvo, AlvoDaForja::Equipado(_)) {
                "vestida"
            } else {
                "na bolsa"
            };
            estilo::texto(
                d.x + u(74.0),
                d.y + u(46.0),
                &format!("{} · {onde}", grau.nome()),
                14,
                estilo::SUAVE,
            );
            let y = d.y + u(100.0);
            let agora_prox = if i.no_topo {
                format!("+{} (no topo)", i.nivel)
            } else {
                format!("+{}  ›  +{}", i.nivel, i.nivel + 1)
            };
            estilo::texto(d.x, y, &agora_prox, 26, estilo::OURO);
            // Quanto de poder o proximo nivel da': sem isto o refino parecia
            // nao fazer nada.
            if !i.no_topo {
                let mut prox = *inst;
                prox.refinement += 1;
                let (a, b) = (
                    crate::bolsa::poder_da_instancia(inst),
                    crate::bolsa::poder_da_instancia(&prox),
                );
                let t = format!("Poder {a} › {b}  (+{})", b - a);
                estilo::texto(d.x + d.w - estilo::medir(&t, 15), y - u(2.0), &t, 15, VERDE);
            }
            if !i.no_topo {
                let cor = if i.chance >= 80 {
                    VERDE
                } else if i.chance >= 30 {
                    AMARELO
                } else {
                    VERMELHO
                };
                estilo::texto(d.x, y + u(34.0), &format!("Chance: {}%", i.chance), 17, cor);
                let (ds, cu) = (tem(slots, item_id::DARKSTEEL), tem(slots, item_id::COPPER));
                estilo::texto(
                    d.x,
                    y + u(62.0),
                    &format!("Darksteel {ds}/{}", i.darksteel),
                    15,
                    if ds >= i.darksteel { VERDE } else { VERMELHO },
                );
                estilo::texto(
                    d.x,
                    y + u(84.0),
                    &format!("Cobre {cu}/{}", i.cobre),
                    15,
                    if cu >= i.cobre { VERDE } else { VERMELHO },
                );
                if crate::onde_obter::botao(Rect::new(d.x + d.w - u(36.0), y + u(44.0), u(34.0), u(22.0))) {
                    self.onde_obter = Some(item_id::DARKSTEEL);
                }
                if crate::onde_obter::botao(Rect::new(d.x + d.w - u(36.0), y + u(68.0), u(34.0), u(22.0))) {
                    self.onde_obter = Some(item_id::COPPER);
                }
                if i.risco {
                    estilo::texto(
                        d.x,
                        y + u(116.0),
                        "Atenção: se falhar, a peça é DESTRUÍDA.",
                        15,
                        VERMELHO,
                    );
                } else {
                    estilo::texto(
                        d.x,
                        y + u(116.0),
                        "Falhar aqui só gasta o material.",
                        14,
                        estilo::SUAVE,
                    );
                }
                let tem_tudo = ds >= i.darksteel && cu >= i.cobre;
                let b = Rect::new(d.x, d.y + d.h - u(50.0), d.w, u(40.0));
                if crate::ui::botao(
                    b,
                    if i.risco {
                        "Refinar (arriscado)"
                    } else {
                        "Refinar"
                    },
                    tem_tudo,
                ) {
                    // ARRISCADO PERGUNTA. Seguro vai direto — pedir "tem
                    // certeza?" para uma falha que só come material seria
                    // treinar o jogador a dizer sim sem ler, e aí a pergunta
                    // que importa não seria lida também.
                    if i.risco {
                        self.confirmar = Some((*alvo, *id, *inst));
                    } else {
                        pedido = Some(ClientMessage::Refinar { alvo: *alvo });
                    }
                }
            }
        }
        if let Some((_, txt, cor, t)) = &self.aviso {
            if agora - t < 4.0 {
                estilo::texto_centro(p.x + p.w * 0.5, p.y + p.h - u(6.0), txt, 15, *cor);
            }
        }
        // A pergunta vem POR CIMA de tudo, e é a última coisa desenhada: ela
        // tem que receber o toque antes de qualquer botão do painel.
        if let Some((alvo, id, inst)) = self.confirmar {
            match confirma_refino(&nome(id), &info(&inst)) {
                Some(true) => {
                    self.confirmar = None;
                    pedido = Some(ClientMessage::Refinar { alvo });
                }
                Some(false) => self.confirmar = None,
                None => {}
            }
        }
        pedido
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peca(nivel: u8) -> ItemInstance {
        let mut i = ItemInstance::roll_for(item_id::KATANA, 5, || 0.5).unwrap();
        i.refinement = nivel;
        i
    }

    #[test]
    fn info_mostra_chance_custo_e_risco_do_proximo() {
        let i = info(&peca(3));
        assert_eq!((i.chance, i.risco, i.no_topo), (80, false, false));
        assert_eq!((i.darksteel, i.cobre), forja::custo_em_jogo(Grau::Comum));
        let i = info(&peca(5));
        assert_eq!((i.chance, i.risco), (30, true), "o +6 arrisca a peca");
        let i = info(&peca(forja::REFINO_MAX));
        assert!(i.no_topo && !i.risco);
    }

    /// A PERGUNTA aparece exatamente onde há destruição, e em lugar nenhum
    /// mais.
    ///
    /// `risco` é quem decide se o toque abre a confirmação ou manda refinar
    /// direto. Perguntar "tem certeza?" numa falha que só come material
    /// treinaria o jogador a dizer sim sem ler — e aí a pergunta que importa
    /// também não seria lida. Não perguntar do +6 em diante é perder a peça
    /// num toque de rotina, que foi o pedido do dono.
    #[test]
    fn so_o_refino_que_destroi_pede_confirmacao() {
        for nivel in 0..forja::REFINO_MAX {
            let i = info(&peca(nivel));
            let destroi = nivel + 1 > forja::REFINO_SEGURO;
            assert_eq!(
                i.risco, destroi,
                "+{nivel} → +{}: risco={} mas destrói={destroi}",
                nivel + 1,
                i.risco
            );
        }
        // No topo não há próximo nível, então não há o que perguntar.
        assert!(!info(&peca(forja::REFINO_MAX)).risco);
        // E a conta que a janela mostra: os dois lados do mesmo número.
        let i = info(&peca(5));
        assert_eq!((i.chance, 100 - i.chance), (30, 70));
    }

    #[test]
    fn resultado_destruido_solta_a_selecao_e_diz_o_que_houve() {
        let mut f = Forja::default();
        f.sel = Some(AlvoDaForja::Bolsa(2));
        f.resultado(resultado::DESTRUIU, 0, "Katana", "", 1.0);
        assert!(f.sel.is_none());
        let (t, _) = texto_do_resultado(resultado::SUBIU, 4, "Katana", "");
        assert!(t.contains("+4"));
        let (t, _) = texto_do_resultado(
            resultado::SEM_MATERIAL,
            1,
            "Katana",
            "precisa de 300 Darksteel e 100 Cobre",
        );
        assert!(t.contains("300 Darksteel"));
    }

    #[test]
    fn so_peca_com_instancia_entra_e_vestida_vem_antes() {
        let mut equip = Equipment::default();
        equip.set(EquipSlot::Weapon, Some(item_id::KATANA), Some(peca(2)));
        let slots = vec![
            InventorySlot {
                item_id: item_id::COPPER,
                qty: 50,
                instance: None,
            },
            InventorySlot {
                item_id: item_id::BRINCO,
                qty: 1,
                instance: Some(peca(0)),
            },
        ];
        let v = pecas(&slots, &equip);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, AlvoDaForja::Equipado(EquipSlot::Weapon));
        assert_eq!(v[1].0, AlvoDaForja::Bolsa(1));
    }
}
