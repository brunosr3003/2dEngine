//! A BOLSA: inventario e equipamento, no molde do MIR4.
//!
//! O MIR4 abre a bolsa como uma tela so', em duas metades. A ESQUERDA e' o
//! personagem, com o que ele esta' vestindo em volta e o poder embaixo; a
//! DIREITA e' a grade de itens, separada por abas, com a ocupacao e o botao de
//! organizar no pe'. Tocar num item abre o cartao dele — nome na cor do grau,
//! tier, refino, atributos, comparacao com o que esta' vestido e a acao — e
//! dois toques fazem a acao direto. Aqui e' a mesma coisa, com o mouse.
//!
//! Nada aqui decide. Equipar e' `UseItem` (o servidor troca com o que estava
//! no slot e cuida do escudo quando a arma muda); desequipar e' um
//! `InventorySwap` pro primeiro espaco vazio. O que volta e' o
//! `InventoryUpdate` / `StatsUpdate` de sempre.

use std::collections::HashMap;

use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use macroquad::prelude::*;
use shared::items::ItemInstance;
use shared::protocol::{ClientMessage, InvSpot};
use shared::skills::Conjunto;
use shared::{item_id, EquipSlot, InventorySlot, PlayerStats};

use crate::hud_estilo::u;
use crate::render3d;
use crate::ui;
use crate::vox::VoxCache;


// Capture dimensions agree with the HUD; normal gameplay uses the window.
fn screen_width() -> f32 { render3d::tela().0 }
fn screen_height() -> f32 { render3d::tela().1 }

const COLUNAS: usize = 8;
const VAO: f32 = 6.0;

const FUNDO: Color = crate::hud_estilo::FUNDO;
const SECAO: Color = crate::hud_estilo::FUNDO_ALTO;
const VAZIA: Color = crate::hud_estilo::FUNDO_BAIXO;
const BORDA: Color = crate::hud_estilo::BORDA;
const TEXTO: Color = crate::hud_estilo::TEXTO;
const APAGADO: Color = crate::hud_estilo::SUAVE;
const VERDE: Color = crate::hud_estilo::VERDE;
const VERMELHO: Color = crate::hud_estilo::VERMELHO;

/// Os slots em volta do retrato, como no MIR4: o que se empunha e se veste
/// de um lado, os acessorios do outro (docs/COMBATE.md).
const ESQUERDA: [(EquipSlot, &str); 6] = [
    (EquipSlot::Weapon, "Weapon"),
    (EquipSlot::Offhand, "Off-hand"),
    (EquipSlot::Armor, "Armour"),
    (EquipSlot::Pet, "Pet"),
    (EquipSlot::Montaria, "Mount"),
    (EquipSlot::AcessorioMontaria, "Mount acc."),
];
const DIREITA: [(EquipSlot, &str); 7] = [
    (EquipSlot::Earring, "Earring"),
    (EquipSlot::Necklace, "Amulet"),
    (EquipSlot::Bracelet, "Bracelet"),
    (EquipSlot::Belt, "Belt"),
    (EquipSlot::Pet2, "Pet 2"),
    (EquipSlot::Pet3, "Pet 3"),
    (EquipSlot::AcessorioPet, "Pet acc."),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Sel {
    Inv(usize),
    Equip(EquipSlot),
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Aba {
    #[default]
    Tudo,
    Equip,
    Consumivel,
    Material,
}

const ABAS: [(Aba, &str); 4] = [
    (Aba::Tudo, "All"),
    (Aba::Equip, "Gear"),
    (Aba::Consumivel, "Consumable"),
    (Aba::Material, "Material"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Acao {
    Equipar(usize),
    Usar(usize),
    Abrir10(usize),
    Desequipar(EquipSlot),
    Organizar,
    /// +10 espacos, em ouro (`shared::armazem`).
    Expandir,
}

/// O que o jogador tem, como o servidor contou, e o estado da tela.
pub struct Bolsa {
    pub aberta: bool,
    pub slots: Vec<InventorySlot>,
    pub equip: shared::Equipment,
    /// The player's packed look (`EntityMeta::aparencia`), set by `main`
    /// before drawing: the portrait wears what the world shows.
    pub aparencia: u32,
    /// Weapon and mount skins (`EntityMeta::skins`), for the same portrait.
    pub skins: u32,
    pub stats: Option<PlayerStats>,
    /// Nome de cada item (`ServerMessage::ItemsConfig`).
    pub nomes: HashMap<u16, String>,
    pub ouro: u64,
    /// Energia de habilidades: saldo próprio, fora da grade e da carteira.
    pub energia: u64,
    pub nivel: u32,
    aba: Aba,
    sel: Option<Sel>,
    /// Ultimo clique: pra reconhecer o duplo.
    clique: (f64, Option<Sel>),
    aviso: Option<(String, f64)>,
    /// Lupa tocada no cartao do item: o item pro "Onde obter".
    pub onde_obter: Option<u16>,
    /// "Refinar +N" tocado no cartao: a peca pra Forja abrir ja' escolhida.
    pub refinar: Option<shared::protocol::AlvoDaForja>,
    /// "Combine" tocado no cartao: a receita pro Craft abrir ja' escolhida.
    ///
    /// O dono pediu "atalho para combinar, forjar, aprimorar, tudo direto no
    /// item no inventario". Refinar ja' saltava pra Forja com a peca na mao;
    /// faltava o outro lado — do MATERIAL pra receita que o gasta, e da PECA
    /// pra receita que a faz. Quem esta' com o item na tela ja' sabe o que
    /// quer; obriga-lo a fechar, abrir o Craft e procurar de novo e' cobrar
    /// pedagio por uma decisao que ele ja' tomou.
    pub combinar: Option<u16>,
    pub craftar: Option<u16>,
    pub aprimorar: Option<(u16, u8, u8)>,
    slot_vazio: Option<EquipSlot>,
    rolagem_compativeis: crate::rolagem::Rolagem,
    desmantelar: Option<usize>,
    /// Expansoes compradas (`ServerMessage::Armazem`).
    pub extra: u8,
    /// A grade rola: com as expansoes ela passa do painel.
    rolagem: crate::rolagem::Rolagem,
}

impl Default for Bolsa {
    fn default() -> Self {
        Bolsa {
            aberta: false,
            slots: Vec::new(),
            equip: Default::default(),
            aparencia: 0,
            skins: 0,
            stats: None,
            nomes: HashMap::new(),
            ouro: 0,
            energia: 0,
            nivel: 0,
            aba: Aba::Tudo,
            sel: None,
            clique: (0.0, None),
            aviso: None,
            onde_obter: None,
            refinar: None,
            combinar: None,
            craftar: None,
            aprimorar: None,
            slot_vazio: None,
            rolagem_compativeis: Default::default(),
            desmantelar: None,
            extra: 0,
            rolagem: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Peca {
    id: u16,
    qty: u32,
    inst: Option<ItemInstance>,
}

impl Peca {
    /// A cor (grau) — e' ela que pinta a borda da celula.
    ///
    /// Duas fontes, nesta ordem: o proprio `item_id`, pra quem guarda a cor
    /// nele (material colorido, chave e pet — `item_id::cor_de_id`), e a
    /// `ItemInstance`, pra equipamento rolado. Antes so' a instancia contava,
    /// e por isso TODO material aparecia com borda cinza, fosse ele verde,
    /// azul ou roxo.
    fn grau(&self) -> u8 {
        if let Some(cor) = item_id::cor_de_id(self.id) {
            return cor;
        }
        self.inst.map_or(1, |i| i.grau())
    }
    /// O tier dentro da cor (I..IV) — o numero romano do canto.
    fn tier(&self) -> u8 {
        self.inst.map_or(1, |i| i.tier())
    }
    fn refino(&self) -> u8 {
        self.inst.map_or(0, |i| i.refinement)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tipo {
    Arma(Conjunto),
    Slot(EquipSlot),
    /// 0 vida, 1 mana, 2 vigor.
    Pocao(u8),
    Pergaminho,
    /// An appearance skin: using it unlocks it in the wardrobe
    /// (`aparencia::skin_do_item`). There was no way to use one before
    /// 01/10/2026 — a bought Storm Captain could only sit in the bag.
    Skin,
    /// Ração de Pet.
    RacaoPet,
    /// Skill de pet, ou o Removedor.
    SkillPet,
    Ouro,
    Madeira,
    Material,
}

fn tipo(id: u16) -> Tipo {
    if id == item_id::GOLD {
        return Tipo::Ouro;
    }
    if id == item_id::WOOD_T1 {
        return Tipo::Madeira;
    }
    match shared::equip_slot_of(id) {
        Some(EquipSlot::Weapon) => return Tipo::Arma(Conjunto::da_arma(id)),
        Some(s) => return Tipo::Slot(s),
        None => {}
    }
    match id {
        x if x == item_id::HEALTH_POTION || x == item_id::GREATER_HEAL => Tipo::Pocao(0),
        x if x == item_id::MANA_POTION || x == item_id::GREATER_MANA => Tipo::Pocao(1),
        x if x == item_id::STAMINA_POTION => Tipo::Pocao(2),
        x if x == item_id::XP_POTION => Tipo::Pocao(3),
        x if x == item_id::FORTUNA_POTION => Tipo::Pocao(4),
        x if x == item_id::SORTE_POTION => Tipo::Pocao(5),
        x if x == item_id::PERGAMINHO_INVOCA_CHAVE
            || x == item_id::PERGAMINHO_INVOCA_MONTARIA
            || x == item_id::PERGAMINHO_INVOCA_TOMO
            || x == item_id::PERGAMINHO_INVOCA_PET =>
        {
            Tipo::Pergaminho
        }
        x if shared::aparencia::skin_do_item(x).is_some() => Tipo::Skin,
        x if x == item_id::RACAO_DE_PET => Tipo::RacaoPet,
        x if x == item_id::REMOVEDOR_DE_SKILL_PET || item_id::e_skill_de_pet(x) => Tipo::SkillPet,
        _ => Tipo::Material,
    }
}

fn aba_de(t: Tipo) -> Aba {
    match t {
        Tipo::Arma(_) | Tipo::Slot(_) => Aba::Equip,
        Tipo::Pocao(_) | Tipo::Pergaminho | Tipo::RacaoPet | Tipo::SkillPet => Aba::Consumivel,
        _ => Aba::Material,
    }
}

fn nome_do_slot(s: EquipSlot) -> &'static str {
    ESQUERDA
        .iter()
        .chain(DIREITA.iter())
        .find(|(x, _)| *x == s)
        .map_or("?", |(_, n)| n)
}

fn compativel_com_slot(equip: &shared::Equipment, slot: EquipSlot, id: u16) -> bool {
    let destino = shared::equip_slot_of(id);
    if destino != Some(slot)
        && !(destino == Some(EquipSlot::Pet) && matches!(slot, EquipSlot::Pet2 | EquipSlot::Pet3))
    {
        return false;
    }
    if slot == EquipSlot::Offhand {
        let conjunto = shared::skills::Conjunto::da_arma(equip.weapon.unwrap_or(0));
        return shared::skills::Conjunto::da_secundaria(id) == Some(conjunto);
    }
    true
}

fn cor_do_tier(t: u8) -> Color {
    let h = shared::items::tier_color_hex(t).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

fn com_alfa(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

const ROMANO: [&str; 5] = ["I", "II", "III", "IV", "V"];

/// "5226" -> "5,2k". A celula tem 50 px: numero cheio nao cabe.
pub(crate) fn curta(q: u32) -> String {
    if q >= 10_000 {
        format!("{}k", q / 1000)
    } else if q >= 1000 {
        // O decimal do idioma, e nao a virgula fixa: 1,5k em pt e 1.5k em en.
        format!("{:.1}k", q as f32 / 1000.0).replace('.', &shared::idioma::separadores().1.to_string())
    } else {
        q.to_string()
    }
}

/// 1234567 -> "1.234.567" em pt, "1,234,567" em en.
///
/// O ponto era fixo aqui, entao o ingles lia "power 2.708" como dois inteiros
/// e sete decimos. Quem sabe a pontuacao de cada idioma e' `idioma`.
pub(crate) fn milhar(v: u64) -> String {
    shared::idioma::milhar(v)
}

/// Primeiro espaco livre da bolsa, contando os que o servidor ainda nem
/// mandou (a lista pode vir mais curta que a bolsa).
fn primeiro_vazio(slots: &[InventorySlot]) -> Option<usize> {
    let n = slots
        .len()
        .saturating_sub(shared::armazem::CARTEIRA.len())
        .max(shared::INVENTORY_SLOTS.min(slots.len()));
    slots[..n.min(slots.len())]
        .iter()
        .position(|s| s.qty == 0)
        .or((slots.len() < shared::INVENTORY_SLOTS).then_some(slots.len()))
}

/// Espacos da grade da bolsa com `extra` expansoes: a lista do servidor
/// menos a carteira do fim (nunca menos que o tamanho comprado).
pub(crate) fn grade(slots: &[InventorySlot], extra: u8) -> usize {
    let comprado = shared::armazem::tamanho(false, extra);
    let na_lista = slots.len().saturating_sub(shared::armazem::CARTEIRA.len());
    comprado.max(na_lista)
}

/// O "poder" do MIR4: um numero so' que resume a ficha. A conta e' nossa.
pub(crate) fn poder(s: &PlayerStats) -> i32 {
    s.attack_damage * 10
        + s.defense * 8
        + s.hp_max
        + s.mp_max / 2
        + (s.dex + s.wis) * 5
        + (s.crit_chance * 1000.0) as i32
}

fn poder_da_peca(p: &Peca) -> i32 {
    if shared::pets::de_item(p.id).is_some() {
        return poder_dos_pontos(shared::pets::pontos_por_stat_da_instancia(
            p.id,
            p.inst.as_ref(),
        ));
    }
    if shared::montarias::de_item(p.id).is_some() {
        return poder_dos_pontos(shared::montarias::pontos_por_stat_da_instancia(
            p.id,
            p.inst.as_ref(),
        ));
    }
    p.inst.map_or(0, |i| poder_da_instancia(&i))
}

/// O poder que um PET soma. Ele nao tem atributo de item: o que ele da' entra
/// como ponto alocado (docs/PETS.md), entao a conta passa os pontos pela
/// mesma `STAT_POINT_BONUS` do servidor e depois pela formula do `poder`.
pub(crate) fn poder_do_pet(id: u16, d: &shared::items::PetData, af: Option<[u8; 2]>) -> i32 {
    poder_dos_pontos(shared::pets::pontos_por_stat(id, d, af))
}

pub(crate) fn poder_dos_pontos(pontos: [u32; shared::STAT_COUNT]) -> i32 {
    let (mut atk, mut def, mut hp, mut mp, mut dex, mut wis) = (0, 0, 0, 0, 0, 0);
    let mut crit = 0.0f32;
    for (i, pts) in pontos.iter().enumerate() {
        let Some(b) = shared::STAT_POINT_BONUS.get(i) else {
            continue;
        };
        let p = *pts as i32;
        atk += b.attack_damage * p;
        def += b.defense * p;
        hp += b.hp_max * p;
        mp += b.mp_max * p;
        dex += b.dex * p;
        wis += b.wis * p;
        crit += b.crit_chance * *pts as f32;
    }
    atk * 10 + def * 8 + hp + mp / 2 + (dex + wis) * 5 + (crit * 1000.0) as i32
}

/// O poder que uma peca soma, com o refino — a MESMA conta do servidor
/// (`ItemInstance::effective_bonus`) passada pela formula do `poder`.
pub(crate) fn poder_da_instancia(i: &shared::items::ItemInstance) -> i32 {
    let b = i.effective_bonus();
    b.attack_damage * 10 + b.defense * 8 + b.hp_max + b.mp_max / 2 + (b.dex + b.wis) * 5
}

struct Tela {
    painel: Rect,
    esq: Rect,
    dir: Rect,
    cel: f32,
}

/// Escala da bolsa (no celular ela cresce ate' encher a tela).
fn escala() -> f32 {
    crate::hud_estilo::escala_do_painel(1080.0, 680.0)
        .min((screen_width() - 16.0) / 1080.0)
        .min((screen_height() - 16.0) / 680.0)
}

fn tela() -> Tela {
    crate::hud_estilo::no_painel(escala(), tela_na_escala)
}

fn tela_na_escala() -> Tela {
    let (sw, sh) = (screen_width(), screen_height());
    let w = (sw - 16.0).min(u(1080.0));
    let h = (sh - 16.0).min(u(680.0));
    let painel = Rect::new((sw - w) * 0.5, (sh - h) * 0.5, w, h);
    let esq_w = (w * 0.44).floor();
    let esq = Rect::new(painel.x + u(16.0), painel.y + u(56.0), esq_w, h - u(72.0));
    let dx = esq.x + esq.w + u(16.0);
    let dir = Rect::new(dx, esq.y, painel.x + w - u(16.0) - dx, esq.h);
    let cel = ((dir.w - (COLUNAS as f32 - 1.0) * u(VAO)) / COLUNAS as f32)
        .min(u(62.0))
        .floor();
    Tela {
        painel,
        esq,
        dir,
        cel,
    }
}

fn mouse() -> Vec2 {
    mouse_position().into()
}

fn clicou_em(r: Rect) -> bool {
    r.contains(mouse()) && crate::foco::clique()
}

impl Bolsa {
    /// Abre pelo icone do HUD ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.sel = None;
        self.desmantelar = None;
        self.slot_vazio = None;
    }

    pub fn alterna(&mut self) {
        self.aberta = !self.aberta;
        self.sel = None;
        self.desmantelar = None;
        self.slot_vazio = None;
    }

    pub fn fecha(&mut self) {
        self.aberta = false;
        self.sel = None;
        self.desmantelar = None;
        self.slot_vazio = None;
    }

    /// O mouse esta' em cima da bolsa aberta? Entao o clique e' dela, e nao do
    /// mundo. (Fechada ela nao tem botao proprio: o icone e' do HUD.)
    pub fn pega_o_mouse(&self) -> bool {
        self.aberta && tela().painel.contains(mouse())
    }

    fn peca(&self, s: Sel) -> Option<Peca> {
        match s {
            Sel::Inv(i) => self.slots.get(i).filter(|x| x.qty > 0).map(|x| Peca {
                id: x.item_id,
                qty: x.qty,
                inst: x.instance,
            }),
            Sel::Equip(slot) => self.equip.get(slot).map(|id| Peca {
                id,
                qty: 1,
                inst: self.equip.get_inst(slot),
            }),
        }
    }

    pub(crate) fn nome(&self, id: u16) -> String {
        self.nomes
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("item {id}"))
    }

    fn acao_padrao(&self, s: Sel) -> Option<Acao> {
        match s {
            Sel::Equip(slot) => Some(Acao::Desequipar(slot)),
            Sel::Inv(i) => match tipo(self.peca(s)?.id) {
                Tipo::Arma(_) | Tipo::Slot(_) => Some(Acao::Equipar(i)),
                Tipo::Pocao(_) | Tipo::Pergaminho | Tipo::Skin => Some(Acao::Usar(i)),
                _ => None,
            },
        }
    }

    /// Seleciona; no segundo clique rapido no mesmo item, faz a acao.
    fn clica(&mut self, s: Sel) -> Option<Acao> {
        let agora = get_time();
        let duplo = self.clique.1 == Some(s) && agora - self.clique.0 < 0.35;
        self.clique = (agora, Some(s));
        self.sel = Some(s);
        if duplo {
            self.acao_padrao(s)
        } else {
            None
        }
    }

    fn pedido(&mut self, a: Acao) -> Option<ClientMessage> {
        self.sel = None;
        match a {
            Acao::Equipar(i) | Acao::Usar(i) => Some(ClientMessage::UseItem { slot: i as u16 }),
            Acao::Abrir10(i) => Some(ClientMessage::AbrirPergaminhos {
                slot: i as u16,
                quantidade: 10,
            }),
            Acao::Desequipar(s) => match primeiro_vazio(&self.slots) {
                Some(i) => Some(ClientMessage::InventorySwap {
                    a: InvSpot::Equip(s),
                    b: InvSpot::Inv(i as u16),
                }),
                None => {
                    self.aviso = Some(("Your bag is full.".into(), get_time()));
                    None
                }
            },
            Acao::Organizar => Some(ClientMessage::InventoryAutoArrange),
            Acao::Expandir => Some(ClientMessage::ExpandirArmazem { banco: false }),
        }
    }

    /// Desenha a bolsa aberta. Devolve o pedido pro servidor, se o jogador fez
    /// alguma coisa.
    pub fn desenha(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        receitas: &[shared::protocol::CraftRecipeNet],
    ) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        crate::hud_estilo::no_painel(escala(), || self.desenha_na_escala(vox, solido, receitas))
    }

    fn desenha_na_escala(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        receitas: &[shared::protocol::CraftRecipeNet],
    ) -> Option<ClientMessage> {
        let t = tela();
        let p = t.painel;
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            Color::new(0.0, 0.0, 0.0, 0.35),
        );
        crate::hud_estilo::ret_arredondado(p, crate::hud_estilo::RAIO, FUNDO);
        crate::hud_estilo::painel_destaque(p, ui::OURO);
        crate::hud_estilo::separador(p.x + u(16.0), p.y + u(46.0), p.w - u(32.0));
        ui::texto(p.x + u(22.0), p.y + u(33.0), "BAG", 26, ui::OURO);
        // Duas linhas de saldos: assim a Energia cabe no celular sem virar
        // item da grade nem empurrar o titulo da bolsa para fora do painel.
        let linha_1 = format!(
            "Gold {}    Copper {}",
            milhar(self.ouro),
            milhar(self.moeda(item_id::COPPER)),
        );
        let linha_2 = format!(
            "Darksteel {}    Energy {}",
            milhar(self.moeda(item_id::DARKSTEEL)),
            milhar(self.energia),
        );
        let largura =
            crate::hud_estilo::medir(&linha_1, 14).max(crate::hud_estilo::medir(&linha_2, 14));
        let x = (p.x + p.w - u(65.0) - largura).max(p.x + u(120.0));
        ui::texto(x, p.y + u(20.0), &linha_1, 14, ui::OURO_CLARO);
        ui::texto(x, p.y + u(40.0), &linha_2, 14, crate::hud_estilo::ACENTO);
        crate::hud_estilo::icone_energia(vec2(x - u(13.0), p.y + u(34.0)), u(18.0));
        if ui::botao(
            Rect::new(p.x + p.w - u(50.0), p.y + u(9.0), u(34.0), u(30.0)),
            "x",
            true,
        ) {
            self.fecha();
            return None;
        }
        if let Some(slot) = self.slot_vazio {
            return self.lista_do_slot(p, slot, vox, solido);
        }

        let cartao = Rect::new(
            t.esq.x + u(18.0),
            t.esq.y + u(44.0),
            t.esq.w - u(36.0),
            (t.esq.h - u(60.0)).min(u(380.0)),
        );
        let bloqueio = self.sel.map(|_| cartao);
        let mut acao = self.desenha_equipamento(t.esq, vox, solido, bloqueio);
        if let Some(a) = self.desenha_grade(t.dir, t.cel, Some((vox, solido))) {
            acao = Some(a);
        }
        if let Some(sel) = self.sel {
            match self.peca(sel) {
                Some(peca) => {
                    if let Some(a) =
                        self.desenha_cartao(cartao, sel, peca, Some((vox, solido)), receitas)
                    {
                        acao = Some(a);
                    }
                }
                None => self.sel = None,
            }
        }
        if let Some((msg, quando)) = &self.aviso {
            if get_time() - quando < 2.5 {
                ui::texto_centro(p.x + p.w * 0.5, p.y + p.h - u(10.0), msg, 18, VERMELHO);
            } else {
                self.aviso = None;
            }
        }
        if let Some(slot) = self.desmantelar {
            return self.confirma_desmantelar(p, slot, receitas);
        }
        acao.and_then(|a| self.pedido(a))
    }

    fn lista_do_slot(
        &mut self,
        painel: Rect,
        slot: EquipSlot,
        vox: &VoxCache,
        solido: &Material,
    ) -> Option<ClientMessage> {
        let w = u(480.0).min(painel.w - u(24.0));
        let h = u(430.0).min(painel.h - u(24.0));
        let r = Rect::new(
            painel.center().x - w * 0.5,
            painel.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_estilo::painel_destaque(r, ui::OURO_CLARO);
        ui::texto(
            r.x + u(18.0),
            r.y + u(34.0),
            &format!("Equip: {}", nome_do_slot(slot)),
            21,
            ui::OURO_CLARO,
        );
        if ui::botao(
            Rect::new(r.x + r.w - u(50.0), r.y + u(8.0), u(36.0), u(34.0)),
            "X",
            true,
        ) {
            self.slot_vazio = None;
            return None;
        }
        let itens: Vec<(usize, InventorySlot)> = self
            .slots
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, item)| {
                item.qty == 1 && compativel_com_slot(&self.equip, slot, item.item_id)
            })
            .collect();
        let area = Rect::new(r.x + u(14.0), r.y + u(52.0), r.w - u(28.0), r.h - u(66.0));
        if itens.is_empty() {
            crate::hud_estilo::texto_ajustado(
                "No compatible item in your bag.",
                area.x + u(8.0),
                area.y + u(30.0),
                area.w - u(16.0),
                16,
                APAGADO,
            );
            return None;
        }
        let passo = u(58.0);
        let total = itens.len() as f32 * passo;
        let clique = self.rolagem_compativeis.quadro(area, total, passo);
        let mut pedido = None;
        crate::rolagem::recortar(Some(area));
        for (n, (i, item)) in itens.iter().enumerate() {
            let linha = Rect::new(
                area.x,
                area.y + n as f32 * passo - self.rolagem_compativeis.pos,
                area.w - u(12.0),
                u(52.0),
            );
            if linha.y + linha.h < area.y || linha.y > area.y + area.h {
                continue;
            }
            let liberado = item
                .instance
                .and_then(|inst| inst.level_req)
                .is_none_or(|nivel| self.nivel >= nivel as u32);
            crate::hud_estilo::cartao(linha, false, clique.is_some_and(|p| linha.contains(p)));
            celula(
                Rect::new(linha.x + u(5.0), linha.y + u(4.0), u(44.0), u(44.0)),
                Some(Peca {
                    id: item.item_id,
                    qty: item.qty,
                    inst: item.instance,
                }),
                false,
                None,
                Some((vox, solido)),
            );
            crate::hud_estilo::texto_ajustado(
                &self.nome(item.item_id),
                linha.x + u(58.0),
                linha.y + u(29.0),
                linha.w - u(100.0),
                16,
                if liberado { TEXTO } else { APAGADO },
            );
            if !liberado {
                let req = item.instance.and_then(|inst| inst.level_req).unwrap_or(0);
                ui::texto(
                    linha.x + linha.w - u(82.0),
                    linha.y + u(30.0),
                    &format!("Level {req}"),
                    13,
                    VERMELHO,
                );
            }
            if liberado && clique.is_some_and(|p| linha.contains(p) && area.contains(p)) {
                pedido = Some(ClientMessage::InventorySwap {
                    a: InvSpot::Inv(*i as u16),
                    b: InvSpot::Equip(slot),
                });
                self.slot_vazio = None;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem_compativeis.desenha(area, total);
        pedido
    }

    // ── a metade do personagem ──

    fn desenha_equipamento(
        &mut self,
        r: Rect,
        vox: &VoxCache,
        solido: &Material,
        bloqueio: Option<Rect>,
    ) -> Option<Acao> {
        crate::hud_estilo::cartao(r, false, false);
        ui::texto(r.x + u(14.0), r.y + u(24.0), "Gear", 20, ui::OURO);
        if self.nivel > 0 {
            let n = format!("Level {}", self.nivel);
            let d = crate::hud_estilo::medir_dim(&n, 17);
            ui::texto(r.x + r.w - u(14.0) - d.width, r.y + u(24.0), &n, 17, TEXTO);
        }
        // O que esta' na mao — a pergunta que a bolsa existe pra responder.
        let arma = self.equip.weapon;
        let em_uso = match arma {
            Some(id) => format!(
                "In use: {} · {}",
                Conjunto::da_arma(id).nome(),
                self.nome(id)
            ),
            None => "In use: no weapon".to_string(),
        };
        ui::texto(r.x + u(14.0), r.y + u(44.0), &em_uso, 16, ui::OURO_CLARO);

        let s = ((r.h - u(60.0) - u(170.0)) / 7.0 - u(16.0))
            .clamp(u(32.0), u(64.0))
            .floor();
        let passo = s + u(16.0);
        let y0 = r.y + u(60.0);
        let xe = r.x + u(14.0);
        let xd = r.x + r.w - u(14.0) - s;
        let retrato = Rect::new(
            xe + s + u(12.0),
            y0,
            xd - u(12.0) - (xe + s + u(12.0)),
            7.0 * passo - u(16.0),
        );
        self.desenha_retrato(retrato, vox, solido);

        let mut acao = None;
        let colunas = [(xe, &ESQUERDA[..]), (xd, &DIREITA[..])];
        let mut clicado = None;
        for (x, lista) in colunas {
            for (k, (slot, rotulo)) in lista.iter().enumerate() {
                let c = Rect::new(x, y0 + k as f32 * passo, s, s);
                let peca = self.peca(Sel::Equip(*slot));
                let sel = self.sel == Some(Sel::Equip(*slot));
                celula(c, peca, sel, Some(*slot), Some((vox, solido)));
                ui::texto_centro(c.x + s * 0.5, c.y + s + u(12.0), rotulo, 12, APAGADO);
                let livre = bloqueio.map_or(true, |b| !b.contains(mouse()));
                if livre && clicou_em(c) {
                    if peca.is_some() {
                        clicado = Some(Sel::Equip(*slot));
                    } else {
                        self.slot_vazio = Some(*slot);
                        self.rolagem_compativeis.zera();
                        self.sel = None;
                    }
                }
            }
        }
        if let Some(sel) = clicado {
            acao = self.clica(sel);
        }

        // O poder e a ficha, embaixo do retrato.
        let mut y = y0 + 7.0 * passo + u(8.0);
        if let Some(st) = &self.stats {
            ui::texto_centro(r.x + r.w * 0.5, y + u(12.0), "POWER", 14, APAGADO);
            ui::texto_centro(
                r.x + r.w * 0.5,
                y + u(42.0),
                &milhar(poder(st).max(0) as u64),
                32,
                ui::OURO,
            );
            y += u(62.0);
            let linhas = [
                ("Attack", st.attack_damage.to_string()),
                ("Defence", st.defense.to_string()),
                ("Health", st.hp_max.to_string()),
                ("Mana", st.mp_max.to_string()),
                ("Dexterity", st.dex.to_string()),
                ("Wisdom", st.wis.to_string()),
                ("Critical", format!("{:.1}%", st.crit_chance * 100.0)),
                (
                    "Atk. speed",
                    format!("{:.0}%", st.attack_speed_mult * 100.0),
                ),
            ];
            let col_w = (r.w - u(28.0)) * 0.5;
            for (i, (rot, val)) in linhas.iter().enumerate() {
                let cx = r.x + u(14.0) + (i % 2) as f32 * col_w;
                let cy = y + (i / 2) as f32 * u(20.0);
                if cy > r.y + r.h - u(6.0) {
                    break;
                }
                ui::texto(cx, cy, rot, 16, APAGADO);
                let d = crate::hud_estilo::medir_dim(val, 16);
                ui::texto(cx + col_w - u(16.0) - d.width, cy, val, 16, TEXTO);
            }
        }
        acao
    }

    /// O personagem girando devagar, com a arma na mao — como o MIR4 mostra
    /// na bolsa. Desenhado DIRETO na tela, num viewport do retrato — sem
    /// render target com profundidade, que o iPhone recusa (ver
    /// `render3d::viewport_em_pixels`). O depth do mundo nao esconde o boneco
    /// porque a profundidade e' limpa antes dele.
    fn desenha_retrato(&mut self, r: Rect, vox: &VoxCache, solido: &Material) {
        if r.w < 20.0 || r.h < 20.0 {
            return;
        }
        // um fundo atras, e o chao em que ele pisa
        draw_rectangle(r.x, r.y, r.w, r.h, crate::hud_estilo::FUNDO_BAIXO);
        let Some(corpo) = vox.rig(render3d::RIG_CORPO) else {
            ui::texto_centro(
                r.x + r.w * 0.5,
                r.y + r.h * 0.5,
                "vox faltando: personagem/corpo.vox",
                12,
                APAGADO,
            );
            return;
        };
        let Some(vp) = render3d::viewport_na_tela(r) else {
            ui::texto_centro(
                r.x + r.w * 0.5,
                r.y + r.h * 0.5,
                "No room for the portrait",
                12,
                APAGADO,
            );
            return;
        };
        let cam = Camera3D {
            position: vec3(0.0, 1.15, 4.6),
            target: vec3(0.0, 0.92, 0.0),
            up: Vec3::Y,
            fovy: 30f32.to_radians(),
            aspect: Some(vp.2 as f32 / vp.3 as f32),
            viewport: Some(vp),
            render_target: render3d::alvo(),
            ..Default::default()
        };
        set_camera(&cam);
        render3d::limpa_so_profundidade();
        gl_use_material(solido);
        let conjunto = Conjunto::da_arma(self.equip.weapon.unwrap_or(0)) as u8;
        let entrada = crate::rig::Entrada {
            fase: 0.0,
            andar: 0.0,
            correr: 0.0,
            tempo: get_time() as f32,
            ar: 0.0,
            degrau: [0.0, 0.0],
            combate: crate::rig::Combate {
                conjunto,
                sacada: 1.0,
                ..Default::default()
            },
        };
        let base = Mat4::from_rotation_y((get_time() as f32 * 0.5).sin() * 0.9);
        let pose = crate::rig::pose(&entrada);
        // The REAL look — face, hair, colours, outfit/skin — and the auras of
        // the equipped pieces, like the world draws this player. It used to
        // be the bare default body with the default hat, so a skin or a blue
        // weapon never showed here.
        let mut veste = render3d::vestimenta_de(vox, self.aparencia).unwrap_or_else(|| {
            let mut v = render3d::Vestimenta::nua(corpo);
            v.cabelo = vox.rig(render3d::RIG_CHAPEU);
            v
        });
        veste.skins = self.skins;
        let auras = shared::auras::equipamento(&self.equip);
        render3d::desenha_rig_com_auras(base, &pose, &veste, vox, auras);
        gl_use_default_material();
        render3d::camera_padrao();
        // a plataforma dourada do MIR4, em 2D por cima do pe'
        let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.90);
        draw_ellipse_lines(
            cx,
            cy,
            r.w * 0.30,
            r.w * 0.06,
            0.0,
            1.5,
            com_alfa(ui::OURO, 0.55),
        );
        crate::hud_estilo::borda_arredondada(
            r,
            crate::hud_estilo::RAIO_PEQUENO + 2.0,
            1.0,
            crate::hud_estilo::BORDA_FORTE,
        );
    }

    // ── a metade dos itens ──

    fn desenha_grade(
        &mut self,
        r: Rect,
        cel: f32,
        palco: Option<(&VoxCache, &Material)>,
    ) -> Option<Acao> {
        let mut acao = None;
        // abas
        let aba_w = (r.w - 3.0 * u(6.0)) / 4.0;
        for (k, (aba, rotulo)) in ABAS.iter().enumerate() {
            let a = Rect::new(r.x + k as f32 * (aba_w + u(6.0)), r.y, aba_w, u(32.0));
            let ativa = self.aba == *aba;
            let sobre = a.contains(mouse());
            crate::hud_estilo::aba(a, rotulo, ativa, sobre);
            if clicou_em(a) {
                self.aba = *aba;
                self.sel = None;
            }
        }

        // Quais espacos aparecem: em "All", a bolsa inteira na ordem dela; nas
        // outras abas, so' os itens daquela categoria, juntos no comeco.
        let tamanho = self.tamanho();
        let mut mostrar: Vec<Option<usize>> = if self.aba == Aba::Tudo {
            (0..tamanho).map(Some).collect()
        } else {
            (0..tamanho.min(self.slots.len()))
                .filter(|&i| {
                    self.slots[i].qty > 0 && aba_de(tipo(self.slots[i].item_id)) == self.aba
                })
                .map(Some)
                .collect()
        };
        let passo = cel + u(VAO);
        let area = Rect::new(r.x, r.y + u(44.0), r.w, r.h - u(44.0) - u(50.0));
        // Enche a area mesmo com a aba quase vazia (a grade nao "encolhe").
        let cabem = ((area.h + u(VAO)) / passo).floor().max(1.0) as usize * COLUNAS;
        mostrar.resize(mostrar.len().max(cabem).div_ceil(COLUNAS) * COLUNAS, None);
        let total = (mostrar.len() / COLUNAS) as f32 * passo;
        // Rola arrastando (dedo), pela roda ou pela barra; o toque na celula
        // vale no SOLTAR, senao o arrasto que comeca nela ja' selecionava.
        let clique = self.rolagem.quadro(area, total, passo);
        let mut clicado = None;
        crate::rolagem::recortar(Some(area));
        for (k, onde) in mostrar.iter().enumerate() {
            let (col, lin) = (k % COLUNAS, k / COLUNAS);
            let c = Rect::new(
                r.x + col as f32 * passo,
                area.y + lin as f32 * passo - self.rolagem.pos,
                cel,
                cel,
            );
            if c.y + c.h < area.y || c.y > area.y + area.h {
                continue;
            }
            let peca = onde.and_then(|i| self.peca(Sel::Inv(i)));
            let sel = onde.is_some() && self.sel == onde.map(Sel::Inv);
            // Celula de ENCHIMENTO (`onde == None`): ela existe so' pra grade
            // nao encolher quando a aba tem pouca coisa, e estava sendo
            // desenhada IGUAL a um espaco vazio de verdade. O jogador via
            // sessenta quadradinhos, o rodape dizia 12/40, e os dois eram a
            // mesma tela — "visualmente parece ter mais do que tem de fato".
            //
            // Espaco que se tem e espaco que nao existe precisam ser
            // distinguiveis, senao o botao de AUMENTAR nao quer dizer nada.
            match onde {
                Some(_) => celula(c, peca, sel, None, palco),
                None => celula_fora(c),
            }
            if let (Some(i), Some(_)) = (onde, peca) {
                if clique.is_some_and(|p| c.contains(p) && area.contains(p)) {
                    clicado = Some(Sel::Inv(*i));
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(s) = clicado {
            acao = self.clica(s);
        }

        // o pe': ocupacao, aumentar e organizar
        let ocupados = self.slots[..tamanho.min(self.slots.len())]
            .iter()
            .filter(|s| s.qty > 0)
            .count();
        let pe = r.y + r.h - u(40.0);
        ui::texto(
            r.x,
            pe + u(22.0),
            &format!("{ocupados}/{tamanho}"),
            20,
            if ocupados >= tamanho { VERMELHO } else { TEXTO },
        );
        let org = Rect::new(r.x + r.w - u(130.0), pe, u(130.0), u(34.0));
        let exp_x = r.x + u(86.0);
        let exp = Rect::new(exp_x, pe, (org.x - u(10.0) - exp_x).max(u(60.0)), u(34.0));
        let custo = shared::armazem::custo(false, self.extra);
        let pode = custo.is_some_and(|c| self.ouro >= c);
        crate::hud_estilo::botao(
            exp,
            &crate::banco_ui::rotulo_de_expandir(false, self.extra),
            crate::hud_estilo::estado_de(exp, !pode, false),
            pode,
        );
        if custo.is_some() && clicou_em(exp) {
            acao = Some(Acao::Expandir);
        }
        if ui::botao(org, "Sort", true) {
            acao = Some(Acao::Organizar);
        }
        acao
    }

    /// Espacos da GRADE: o que as expansoes dao. A lista do servidor tem a
    /// carteira no fim (cobre, darksteel), que nao e' grade.
    fn tamanho(&self) -> usize {
        grade(&self.slots, self.extra)
    }

    /// Quanto da moeda `id` a carteira (e o resto da lista) tem.
    pub fn moeda(&self, id: u16) -> u64 {
        self.slots
            .iter()
            .filter(|s| s.item_id == id && s.instance.is_none())
            .map(|s| s.qty as u64)
            .sum()
    }

    // ── o cartao do item ──

    fn desenha_cartao(
        &mut self,
        r: Rect,
        sel: Sel,
        peca: Peca,
        palco: Option<(&VoxCache, &Material)>,
        receitas: &[shared::protocol::CraftRecipeNet],
    ) -> Option<Acao> {
        let cor = cor_do_tier(peca.grau());
        crate::hud_estilo::ret_arredondado(
            r,
            crate::hud_estilo::RAIO,
            crate::hud_estilo::FUNDO,
        );
        crate::hud_estilo::painel_destaque(r, cor);
        crate::hud_estilo::borda_arredondada(r, crate::hud_estilo::RAIO, 1.0, com_alfa(cor, 0.55));

        let ic = Rect::new(r.x + u(16.0), r.y + u(18.0), u(64.0), u(64.0));
        celula(ic, Some(peca), false, None, palco);
        let tx = ic.x + ic.w + u(14.0);
        let nome = self.nome(peca.id);
        let titulo = if peca.refino() > 0 {
            format!("+{} {nome}", peca.refino())
        } else {
            nome
        };
        ui::texto(tx, r.y + u(40.0), &titulo, 22, cor);
        if !matches!(tipo(peca.id), Tipo::Ouro)
            && crate::onde_obter::botao(Rect::new(
                r.x + r.w - u(52.0),
                r.y + u(14.0),
                u(38.0),
                u(38.0),
            ))
        {
            self.onde_obter = Some(peca.id);
        }
        let t = tipo(peca.id);
        let classe = match t {
            Tipo::Arma(c) => format!("Weapon · {}", c.nome()),
            Tipo::Slot(s) => nome_do_slot(s).to_string(),
            Tipo::Pocao(_) => "Consumable".into(),
            Tipo::Pergaminho => "Summoning Scroll".into(),
            Tipo::Ouro => "Currency".into(),
            _ => "Material".into(),
        };
        ui::texto(tx, r.y + u(62.0), &classe, 16, TEXTO);
        if let Some(i) = peca.inst {
            ui::texto(
                tx,
                r.y + u(80.0),
                &format!(
                    "{} · Tier {} · item level {}",
                    shared::forja::Grau::de_u8(i.grau()).map_or("Common", |g| g.nome()),
                    ROMANO[(i.tier() - 1) as usize],
                    i.item_level
                ),
                15,
                APAGADO,
            );
        } else if peca.qty > 1 {
            ui::texto(
                tx,
                r.y + u(80.0),
                &format!("Quantity {}", milhar(peca.qty as u64)),
                15,
                APAGADO,
            );
        }

        let mut y = r.y + u(112.0);
        crate::hud_estilo::separador(r.x + u(16.0), y - u(12.0), r.w - u(32.0));
        // O pet nao tem atributo de item: o que ele da' entra como PONTO
        // ALOCADO (docs/PETS.md), entao a ficha dele e' outra.
        if let Some((especie, grau)) = shared::pets::de_item(peca.id) {
            let dados = shared::pets::dados(peca.inst.as_ref());
            let af = peca.inst.as_ref().and_then(|i| i.afinidade);
            ui::texto(r.x + u(20.0), y + u(4.0), especie.descricao, 15, APAGADO);
            y += u(26.0);
            let linhas = [
                (
                    "Power",
                    milhar(
                        poder_dos_pontos(shared::pets::pontos_por_stat_da_instancia(
                            peca.id,
                            peca.inst.as_ref(),
                        ))
                        .max(0) as u64,
                    ),
                ),
                (
                    "Level",
                    format!(
                        "{} / {}",
                        shared::pets::nivel_de_xp(dados.xp),
                        shared::pets::NIVEL_MAX
                    ),
                ),
                (
                    "Speed",
                    format!("{:.0}%", shared::pets::velocidade_com(grau, &dados) * 100.0),
                ),
                (
                    "Loot pickup range",
                    format!("{:.0} tiles", shared::pets::raio_com(grau, &dados)),
                ),
            ];
            for (rot, val) in linhas {
                ui::texto(r.x + u(20.0), y + u(4.0), rot, 17, TEXTO);
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
            const SIGLAS: [&str; shared::STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];
            for (i, pts) in shared::pets::pontos_por_stat_da_instancia(peca.id, peca.inst.as_ref())
                .iter()
                .enumerate()
            {
                if *pts == 0 {
                    continue;
                }
                ui::texto(r.x + u(20.0), y + u(4.0), SIGLAS[i], 17, TEXTO);
                let val = format!("+{pts}");
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
        } else if let Some((especie, grau)) = shared::montarias::de_item(peca.id) {
            // A MONTARIA tambem nao tem atributo de item: o que ela da' sao
            // pontos alocados e, principalmente, VELOCIDADE — que era o unico
            // numero em lugar nenhum. O dono: "nao ta mostrando o bonus de
            // mov speed q a mount da no inv".
            //
            // Tres numeros, porque sao tres situacoes que o jogador compara:
            // montado andando, montado ESPOREANDO (o sprint multiplica a
            // montaria desde 22/09/2026) e quanto isso ganha de quem corre a
            // pe'. So' o primeiro seria bonito e pouco util.
            ui::texto(r.x + u(20.0), y + u(4.0), especie.descricao, 15, APAGADO);
            y += u(26.0);
            let v = shared::montarias::velocidade_da_instancia(grau, peca.inst.as_ref());
            let esporeado = v * shared::SPRINT_SPEED_MULT;
            let vs_correr = v / shared::SPRINT_SPEED_MULT - 1.0;
            for (rot, val) in [
                ("Speed", format!("{:.0}%", v * 100.0)),
                ("Spurring", format!("{:.0}%", esporeado * 100.0)),
                ("vs. running on foot", format!("{:+.0}%", vs_correr * 100.0)),
            ] {
                ui::texto(r.x + u(20.0), y + u(4.0), rot, 17, TEXTO);
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
            const SIGLAS_M: [&str; shared::STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];
            let af = peca.inst.as_ref().and_then(|i| i.afinidade);
            for (i, pts) in
                shared::montarias::pontos_por_stat_da_instancia(peca.id, peca.inst.as_ref())
                    .iter()
                    .enumerate()
            {
                if *pts == 0 {
                    continue;
                }
                ui::texto(r.x + u(20.0), y + u(4.0), SIGLAS_M[i], 17, TEXTO);
                let val = format!("+{pts}");
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
        } else if let Some((pet, efeito)) = shared::acessorios::tipo(peca.id) {
            ui::texto(r.x + u(20.0), y + u(4.0), efeito.nome(), 17, VERDE);
            y += u(28.0);
            ui::texto(r.x + u(20.0), y + u(4.0),
                if pet { "Active with a pet equipped" } else { "Active with a mount equipped" }, 15, APAGADO);
        } else if let Some(i) = peca.inst {
            let atributos = [
                ("Attack", i.attack_damage),
                ("Defence", i.defense),
                ("Health", i.hp_max),
                ("Mana", i.mp_max),
                ("Dexterity", i.dex),
                ("Wisdom", i.wis),
            ];
            for (rot, v) in atributos.iter().filter(|(_, v)| *v != 0) {
                ui::texto(r.x + u(20.0), y + u(4.0), rot, 17, TEXTO);
                let val = format!("+{v}");
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
            // A FOR DA ARMADURA MEDIA nao e' atributo do TEMPLATE: ela entra
            // em `effective_stats` como ponto alocado, e por isso nao aparecia
            // em lugar nenhum que se pudesse ler. Do lado de fora isso le'
            // como "a media nao da' forca" — e foi o que o dono leu, duas
            // vezes: uma na ficha e outra montando a peca no Craft.
            let emp = shared::for_da_armadura(peca.id, self.nivel.max(1));
            if emp > 0 {
                ui::texto(r.x + u(20.0), y + u(4.0), "Strength", 17, TEXTO);
                let val = format!("+{emp}");
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - u(20.0) - d.width, y + u(4.0), &val, 17, VERDE);
                y += u(22.0);
            }
            if let Some(req) = i.level_req {
                let falta = (self.nivel as u16) < req && self.nivel > 0;
                ui::texto(
                    r.x + u(20.0),
                    y + u(8.0),
                    &format!("Requires level {req}"),
                    16,
                    if falta { VERMELHO } else { APAGADO },
                );
                y += u(24.0);
            }
            // Comparacao com o que esta' vestido naquele slot — o MIR4 mostra a
            // seta; aqui vai o saldo de poder.
            if let (Sel::Inv(_), Some(slot)) = (sel, shared::equip_slot_of(peca.id)) {
                let vestido = self.peca(Sel::Equip(slot));
                let saldo = poder_da_peca(&peca) - vestido.map_or(0, |v| poder_da_peca(&v));
                let (txt, c) = match saldo {
                    0 => ("same power as equipped".to_string(), APAGADO),
                    s if s > 0 => (format!("+{s} power over what you have on"), VERDE),
                    s => (format!("{s} power over what you have on"), VERMELHO),
                };
                ui::texto(r.x + u(20.0), y + u(10.0), &txt, 16, c);
            }
        } else {
            let txt = match t {
                Tipo::Pocao(0) => "Restores health.",
                Tipo::Pocao(1) => "Restores mana.",
                Tipo::Pocao(3) => "+30% de XP por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(4) => "+30% de ouro e cobre dos bichos por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(5) => "+20% de chance de drop (bichos e coleta) por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(_) => "Restores stamina.",
                Tipo::Pergaminho => "Abra para revelar um prêmio aleatório decidido pelo servidor.",
                Tipo::Arma(_) | Tipo::Slot(_) => "Peça básica, sem instância de atributos.",
                _ => "Crafting material.",
            };
            ui::texto(r.x + u(20.0), y + u(4.0), txt, 16, APAGADO);
        }

        // Cada ação tem seu próprio botão. O atalho aparece mesmo quando ainda
        // faltam materiais ou uma segunda peça; o painel de destino explica o requisito.
        let mut acao = None;
        let mut opcoes: Vec<(&str, u8)> = Vec::new();
        match sel {
            Sel::Equip(_) => opcoes.push(("Unequip", 0)),
            Sel::Inv(_) => match t {
                Tipo::Arma(_) | Tipo::Slot(_) => opcoes.push(("Equip", 0)),
                Tipo::Pocao(_) | Tipo::Pergaminho => opcoes.push(("Use", 0)),
                Tipo::Skin => opcoes.push(("Unlock", 0)),
                _ => {}
            },
        }
        if matches!(sel, Sel::Inv(_)) && matches!(t, Tipo::Pergaminho) && peca.qty >= 10 {
            opcoes.push(("Open 10+1", 1));
        }
        if let Some(inst) = peca.inst {
            if inst.refinement < shared::forja::REFINO_MAX {
                opcoes.push(("Refine", 2));
            }
            if matches!(sel, Sel::Inv(_)) && shared::equip_slot_of(peca.id).is_some() {
                opcoes.push(("Upgrade", 3));
            }
        }
        if matches!(sel, Sel::Inv(_)) {
            if receitas.iter().any(|r| {
                r.output_item_id == peca.id || r.inputs.iter().any(|[id, _]| *id == peca.id as u32)
            }) {
                opcoes.push(("Craft", 4));
            }
            if shared::combinar::receita(peca.id).is_some() {
                opcoes.push(("Combine", 5));
            }
            if peca.inst.is_some() && receita_do_item(receitas, peca.id, peca.grau()).is_some() {
                opcoes.push(("Salvage", 6));
            }
        }
        let colunas = 3usize;
        let bw = (r.w - u(32.0) - u(16.0)) / colunas as f32;
        let linhas = opcoes.len().div_ceil(colunas);
        let inicio_y = r.y + r.h
            - u(16.0)
            - linhas as f32 * u(38.0)
            - (linhas.saturating_sub(1)) as f32 * u(5.0);
        for (n, (rotulo, codigo)) in opcoes.into_iter().enumerate() {
            let coluna = n % colunas;
            let linha = n / colunas;
            let botao = Rect::new(
                r.x + u(16.0) + coluna as f32 * (bw + u(8.0)),
                inicio_y + linha as f32 * u(43.0),
                bw,
                u(38.0),
            );
            if !ui::botao(botao, rotulo, true) {
                continue;
            }
            match (codigo, sel) {
                (0, Sel::Equip(s)) => acao = Some(Acao::Desequipar(s)),
                (0, Sel::Inv(i)) if matches!(t, Tipo::Arma(_) | Tipo::Slot(_)) => {
                    acao = Some(Acao::Equipar(i))
                }
                (0, Sel::Inv(i)) => acao = Some(Acao::Usar(i)),
                (1, Sel::Inv(i)) => acao = Some(Acao::Abrir10(i)),
                (2, Sel::Inv(i)) => {
                    self.refinar = Some(shared::protocol::AlvoDaForja::Bolsa(i as u16));
                    self.sel = None;
                }
                (2, Sel::Equip(s)) => {
                    self.refinar = Some(shared::protocol::AlvoDaForja::Equipado(s));
                    self.sel = None;
                }
                (3, Sel::Inv(_)) => {
                    if let Some(inst) = peca.inst {
                        self.aprimorar = Some((peca.id, inst.grau(), inst.tier()));
                        self.sel = None;
                    }
                }
                (4, Sel::Inv(_)) => {
                    self.craftar = Some(peca.id);
                    self.sel = None;
                }
                (5, Sel::Inv(_)) => {
                    self.combinar = Some(peca.id);
                    self.sel = None;
                }
                (6, Sel::Inv(i)) => self.desmantelar = Some(i),
                _ => {}
            }
        }
        let fechar = Rect::new(r.x + r.w - u(42.0), r.y + u(8.0), u(30.0), u(30.0));
        if ui::botao(fechar, "X", true) {
            self.sel = None;
        }
        acao
    }

    fn confirma_desmantelar(
        &mut self,
        painel: Rect,
        slot: usize,
        receitas: &[shared::protocol::CraftRecipeNet],
    ) -> Option<ClientMessage> {
        let Some(peca) = self.peca(Sel::Inv(slot)) else {
            self.desmantelar = None;
            return None;
        };
        let Some(receita) = receita_do_item(receitas, peca.id, peca.grau()) else {
            self.desmantelar = None;
            return None;
        };
        crate::hud_layout::escurece(0.68);
        let w = u(470.0).min(painel.w - u(20.0));
        let h = u(340.0).min(painel.h - u(20.0));
        let r = Rect::new(
            painel.center().x - w * 0.5,
            painel.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_estilo::painel_destaque(r, ui::OURO_CLARO);
        ui::texto(
            r.x + u(18.0),
            r.y + u(33.0),
            "Salvage item?",
            21,
            ui::OURO_CLARO,
        );
        ui::texto(
            r.x + u(18.0),
            r.y + u(62.0),
            &self.nome(peca.id),
            17,
            crate::hud_estilo::TEXTO,
        );
        ui::texto(
            r.x + u(18.0),
            r.y + u(88.0),
            "A peça será consumida. Você receberá:",
            15,
            crate::hud_estilo::TEXTO,
        );
        let mut y = r.y + u(114.0);
        for (i, &[id, qtd]) in receita.inputs.iter().enumerate() {
            if id == 0 || qtd == 0 {
                continue;
            }
            let texto = if i == 0 {
                format!("{}: 10% chance to recover 1", self.nome(id as u16))
            } else {
                let volta = qtd / 5;
                if volta == 0 {
                    continue;
                }
                format!("{} ×{} (20%)", self.nome(id as u16), volta)
            };
            ui::texto(r.x + u(22.0), y, &texto, 14, crate::hud_estilo::TEXTO);
            y += u(25.0);
        }
        let by = r.y + r.h - u(50.0);
        let cancelar = Rect::new(r.x + u(18.0), by, (r.w - u(54.0)) * 0.5, u(36.0));
        let confirmar = Rect::new(cancelar.x + cancelar.w + u(18.0), by, cancelar.w, u(36.0));
        if ui::botao(cancelar, "Cancel", true) {
            self.desmantelar = None;
        } else if ui::botao(confirmar, "Salvage", true) {
            self.desmantelar = None;
            self.sel = None;
            return Some(ClientMessage::Desmantelar { slot: slot as u16 });
        }
        None
    }
}

fn receita_do_item<'a>(
    receitas: &'a [shared::protocol::CraftRecipeNet],
    item_id: u16,
    grau: u8,
) -> Option<&'a shared::protocol::CraftRecipeNet> {
    receitas.iter().find(|r| {
        r.output_item_id == item_id
            && r.roll_instance
            && r.output_qty == 1
            && r.tier == grau
            && r.id >= shared::receitas::PRIMEIRO_ID
            && r.id < shared::receitas::PRIMEIRO_ID + 400
    })
}

/// Uma celula avulsa, pra quem desenha lista de item fora da bolsa (a faixa
/// de colecao do pet e da montaria). Mesma cara da celula da grade.
pub(crate) fn celula_avulsa(
    r: Rect,
    id: u16,
    qty: u32,
    selecionada: bool,
    palco: Option<(&VoxCache, &Material)>,
) {
    celula(
        r,
        Some(Peca {
            id,
            qty,
            inst: None,
        }),
        selecionada,
        None,
        palco,
    );
}

/// Uma celula de item: fundo na cor do grau, o icone, o tier em romano no
/// canto de cima, o refino do outro lado e a quantidade embaixo. `vazio` e' o
/// slot de equipamento sem nada: aparece a silhueta apagada do que vai ali.
/// Uma celula que NAO E' ESPACO SEU: so' enche a grade.
///
/// Sem borda e quase sem cor — ela tem que ler como "fundo", e nao como
/// "espaco livre". Ver o `match` que a chama.
fn celula_fora(r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.025));
}

fn celula(
    r: Rect,
    peca: Option<Peca>,
    selecionada: bool,
    vazio: Option<EquipSlot>,
    // O palco 3D. `None` desenha a silhueta vetorial — e' o que acontece em
    // teste e em qualquer chamada que nao tenha o cache de modelos a mao.
    palco: Option<(&VoxCache, &Material)>,
) {
    let sobre = r.contains(mouse());
    match peca {
        Some(p) => {
            let cor = cor_do_tier(p.grau());
            crate::hud_estilo::slot(r, Some(cor), sobre, selecionada);
            // O icone do PET e da MONTARIA e' o modelo 3D dele
            // (docs/PETS.md, docs/MONTARIAS.md).
            if !icone_de_bicho(r, p.id, palco) {
                icone_do_item(r, p.id, 1.0);
            }
            let fonte = (r.w * 0.26).clamp(u(11.0), u(16.0)) as u16;
            if p.inst.is_some() {
                ui::texto(
                    r.x + u(4.0),
                    r.y + fonte as f32,
                    ROMANO[(p.tier() - 1) as usize],
                    fonte,
                    cor,
                );
            }
            if p.refino() > 0 {
                let t = format!("+{}", p.refino());
                let d = crate::hud_estilo::medir_dim(&t, fonte);
                ui::texto(
                    r.x + r.w - d.width - u(4.0),
                    r.y + fonte as f32,
                    &t,
                    fonte,
                    ui::OURO_CLARO,
                );
            }
            if p.qty > 1 {
                let t = curta(p.qty);
                let d = crate::hud_estilo::medir_dim(&t, fonte);
                ui::texto(
                    r.x + r.w - d.width - u(3.0),
                    r.y + r.h - u(4.0),
                    &t,
                    fonte,
                    BLACK,
                );
                ui::texto(
                    r.x + r.w - d.width - u(4.0),
                    r.y + r.h - u(5.0),
                    &t,
                    fonte,
                    TEXTO,
                );
            }
        }
        None => {
            crate::hud_estilo::slot(r, None, sobre && vazio.is_none(), false);
            if let Some(s) = vazio {
                let t = if s == EquipSlot::Weapon {
                    Tipo::Arma(Conjunto::EspadaEscudo)
                } else {
                    Tipo::Slot(s)
                };
                icone(r, t, 0, 0.18);
            }
        }
    }
    if selecionada {
        crate::hud_estilo::borda_arredondada(
            Rect::new(r.x - u(3.0), r.y - u(3.0), r.w + u(6.0), r.h + u(6.0)),
            crate::hud_estilo::RAIO_PEQUENO + u(4.0),
            u(2.0),
            ui::OURO,
        );
    }
}

/// O icone de um item (bolsa, loja, barra, craft, forja). A arte e' o atlas
/// de `icones`; o desenho por categoria abaixo so' cobre item sem icone.
pub(crate) fn icone_do_item(r: Rect, id: u16, a: f32) {
    icone_do_item_com(r, id, a, None)
}

/// O icone de um item, com o PALCO 3D quando quem desenha tem um.
///
/// Pet e montaria sao modelo girando na bolsa e eram silhueta chapada na
/// forja e na combinacao — o dono: "o icone dos pets e mounts no combine /
/// forja tem q ser o icone 3d igual do inv". Eram duas caras pro mesmo bicho,
/// e na tela de combinar (onde se arrasta cinco da MESMA cor) duas caras pro
/// mesmo bicho e' o que mais atrapalha.
pub(crate) fn icone_do_item_com(r: Rect, id: u16, a: f32, palco: Option<(&VoxCache, &Material)>) {
    if id == shared::item_id::MOEDA_MAGICA {
        let c = r.center();
        let raio = r.w.min(r.h) * 0.36;
        draw_circle(c.x, c.y, raio, Color::new(0.30, 0.14, 0.55, a));
        draw_circle_lines(
            c.x,
            c.y,
            raio,
            (raio * 0.16).max(1.0),
            Color::new(0.92, 0.72, 1.0, a),
        );
        draw_circle(c.x, c.y, raio * 0.33, Color::new(0.75, 0.42, 1.0, a));
        return;
    }
    if icone_de_bicho(r, id, palco) {
        return;
    }
    if crate::icones::desenha(id, r, a) {
        return;
    }
    icone(r, tipo(id), id, a);
}

/// O icone de um PET ou de uma MONTARIA e' o modelo 3D dele girando
/// (docs/PETS.md, docs/MONTARIAS.md): o bicho na cor do grau. Sem palco
/// (celula pequena demais, modelo ainda carregando, orcamento do quadro
/// estourado) cai na silhueta vetorial.
///
/// A montaria entrou aqui junto com o pet porque a silhueta dela era a mesma
/// para as cinco especies: sem o modelo, o urso laranja e o lobo cinza eram
/// dois quadradinhos iguais de cores diferentes.
///
/// Devolve `false` quando nao desenhou em 3D.
fn icone_de_bicho(r: Rect, id: u16, palco: Option<(&VoxCache, &Material)>) -> bool {
    let Some((vox, solido)) = palco else {
        return false;
    };
    // Um giro lento e preguicoso, com fase por item: dois bichos na mesma
    // tela nao ficam em espelho.
    let giro = get_time() as f32 * 0.5 + id as f32 * 0.7;
    crate::render3d::vitrine_pet(vox, id, r, giro, solido)
        || crate::render3d::vitrine_montaria(vox, id, r, giro, solido)
        || crate::icones::vitrine_skin(vox, id, r, giro, solido)
}

/// O icone de um item, desenhado por categoria. Nao ha arte de icone ainda:
/// a silhueta diz o que a coisa e', e a cor do grau vem da celula.
fn icone(r: Rect, t: Tipo, id: u16, a: f32) {
    let c = r.center();
    let s = r.w * 0.34;
    let k = |r_: f32, g: f32, b: f32| Color::new(r_, g, b, a);
    let aco = k(0.84, 0.87, 0.92);
    let aco_esc = k(0.52, 0.56, 0.62);
    let couro = k(0.52, 0.33, 0.18);
    let ouro = k(0.93, 0.75, 0.30);
    let pano = k(0.62, 0.66, 0.74);
    let linha = |x0: f32, y0: f32, x1: f32, y1: f32, w: f32, cor: Color| {
        draw_line(
            c.x + x0 * s,
            c.y + y0 * s,
            c.x + x1 * s,
            c.y + y1 * s,
            w * s,
            cor,
        )
    };
    match t {
        Tipo::Arma(Conjunto::EspadaEscudo) | Tipo::Arma(Conjunto::Katana) => {
            linha(-0.45, 0.45, 0.95, -0.95, 0.26, aco);
            linha(-0.3, 0.3, 0.8, -0.8, 0.07, aco_esc);
            linha(-0.85, 0.05, -0.05, 0.85, 0.2, ouro);
            linha(-0.45, 0.45, -0.9, 0.9, 0.2, couro);
        }
        Tipo::Arma(Conjunto::Pistolas) => {
            draw_rectangle(c.x - s * 0.9, c.y - s * 0.45, s * 1.8, s * 0.42, aco);
            draw_rectangle(c.x - s * 0.9, c.y - s * 0.45, s * 0.3, s * 0.42, aco_esc);
            linha(0.35, -0.1, 0.75, 0.85, 0.38, couro);
            draw_circle(c.x + s * 0.1, c.y + s * 0.2, s * 0.14, aco_esc);
        }
        Tipo::Arma(Conjunto::AnelMagico) => {
            linha(-0.5, 0.95, 0.35, -0.5, 0.18, couro);
            draw_circle(c.x + s * 0.45, c.y - s * 0.65, s * 0.32, k(0.45, 0.62, 1.0));
            draw_circle(c.x + s * 0.38, c.y - s * 0.72, s * 0.11, k(0.85, 0.92, 1.0));
        }
        Tipo::RacaoPet => {
            // Tigela com ração: um monte de bolinhas dentro.
            let tigela = k(0.55, 0.40, 0.28);
            let racao = k(0.70, 0.45, 0.20);
            for (dx, dy) in [(-0.3, -0.2), (0.05, -0.32), (0.35, -0.18), (0.0, -0.05)] {
                draw_circle(c.x + dx * s, c.y + dy * s, s * 0.17, racao);
            }
            draw_triangle(
                vec2(c.x - s * 0.85, c.y - s * 0.05),
                vec2(c.x + s * 0.85, c.y - s * 0.05),
                vec2(c.x, c.y + s * 0.85),
                tigela,
            );
            draw_rectangle(c.x - s * 0.9, c.y - s * 0.2, s * 1.8, s * 0.2, tigela);
        }
        Tipo::SkillPet => {
            // Pata com um brilho: skill e' o que o bichinho aprende. O
            // Removedor sai em vermelho, com um corte por cima.
            let removedor = id == item_id::REMOVEDOR_DE_SKILL_PET;
            let cor = if removedor {
                k(0.88, 0.35, 0.32)
            } else {
                k(0.55, 0.78, 1.0)
            };
            draw_circle(c.x, c.y + s * 0.3, s * 0.45, cor);
            for dx in [-0.55, -0.18, 0.18, 0.55] {
                draw_circle(c.x + dx * s, c.y - s * 0.4, s * 0.2, cor);
            }
            if removedor {
                linha(-0.9, -0.9, 0.9, 0.9, 0.22, k(0.98, 0.9, 0.9));
            } else {
                draw_circle(c.x + s * 0.62, c.y - s * 0.75, s * 0.14, ouro);
            }
        }
        Tipo::Slot(EquipSlot::Montaria) => {
            // Bicho de porte com sela: e' o que separa da silhueta do pet.
            let cor = shared::montarias::de_item(id)
                .map(|(_, g)| com_alfa(cor_do_tier(g), a))
                .unwrap_or(couro);
            let escuro = Color::new(cor.r * 0.7, cor.g * 0.7, cor.b * 0.7, a);
            crate::hud_estilo::ret_arredondado(
                Rect::new(c.x - s * 0.95, c.y - s * 0.25, s * 1.6, s * 0.8),
                s * 0.3,
                cor,
            );
            draw_circle(c.x + s * 0.95, c.y - s * 0.45, s * 0.4, cor);
            draw_triangle(
                vec2(c.x + s * 0.75, c.y - s * 0.75),
                vec2(c.x + s * 0.88, c.y - s * 1.1),
                vec2(c.x + s * 1.0, c.y - s * 0.72),
                escuro,
            );
            for dx in [-0.75, -0.2, 0.35] {
                draw_rectangle(c.x + dx * s, c.y + s * 0.5, s * 0.24, s * 0.5, escuro);
            }
            // a sela
            crate::hud_estilo::ret_arredondado(
                Rect::new(c.x - s * 0.35, c.y - s * 0.55, s * 0.7, s * 0.34),
                s * 0.12,
                escuro,
            );
            linha(-0.95, -0.2, -1.3, -0.8, 0.18, escuro);
        }
        Tipo::Slot(EquipSlot::Pet) => {
            // Silhueta de bichinho de quatro patas, de lado: so' aparece
            // quando o modelo 3D nao pode ser desenhado.
            let cor = shared::pets::de_item(id)
                .map(|(_, g)| com_alfa(cor_do_tier(g), a))
                .unwrap_or(couro);
            let escuro = Color::new(cor.r * 0.7, cor.g * 0.7, cor.b * 0.7, a);
            // tronco
            crate::hud_estilo::ret_arredondado(
                Rect::new(c.x - s * 0.75, c.y - s * 0.15, s * 1.3, s * 0.7),
                s * 0.3,
                cor,
            );
            // cabeca
            draw_circle(c.x + s * 0.78, c.y - s * 0.3, s * 0.42, cor);
            // orelhas
            draw_triangle(
                vec2(c.x + s * 0.55, c.y - s * 0.6),
                vec2(c.x + s * 0.72, c.y - s * 0.95),
                vec2(c.x + s * 0.85, c.y - s * 0.58),
                escuro,
            );
            draw_triangle(
                vec2(c.x + s * 0.92, c.y - s * 0.62),
                vec2(c.x + s * 1.08, c.y - s * 0.92),
                vec2(c.x + s * 1.12, c.y - s * 0.5),
                escuro,
            );
            // patas
            for i in 0..3 {
                let x = c.x - s * 0.6 + i as f32 * s * 0.55;
                draw_rectangle(x, c.y + s * 0.45, s * 0.22, s * 0.45, escuro);
            }
            // rabo
            linha(-0.75, -0.05, -1.15, -0.65, 0.18, escuro);
            // olho
            draw_circle(c.x + s * 0.92, c.y - s * 0.34, s * 0.09, k(0.08, 0.08, 0.1));
        }
        Tipo::Slot(EquipSlot::Offhand) => {
            // a secundaria do conjunto: manto, bainha ou coldre
            match id {
                item_id::BAINHA => {
                    linha(-0.75, 0.75, 0.75, -0.75, 0.3, k(0.55, 0.12, 0.14));
                    linha(0.55, -0.55, 0.8, -0.8, 0.3, ouro);
                    linha(-0.75, 0.75, -0.6, 0.6, 0.3, ouro);
                }
                item_id::COLDRE => {
                    draw_rectangle(c.x - s * 0.5, c.y - s * 0.6, s * 1.0, s * 1.4, couro);
                    draw_rectangle(c.x - s * 0.55, c.y - s * 0.7, s * 1.1, s * 0.22, ouro);
                    draw_rectangle(c.x - s * 0.15, c.y - s * 0.95, s * 0.3, s * 0.3, aco_esc);
                }
                _ => {
                    let cor = if id == item_id::MANTO_DO_MAGO {
                        k(0.22, 0.28, 0.62)
                    } else {
                        k(0.62, 0.18, 0.18)
                    };
                    draw_triangle(
                        vec2(c.x - s * 0.4, c.y - s * 0.85),
                        vec2(c.x + s * 0.4, c.y - s * 0.85),
                        vec2(c.x - s * 0.9, c.y + s * 0.9),
                        cor,
                    );
                    draw_triangle(
                        vec2(c.x + s * 0.4, c.y - s * 0.85),
                        vec2(c.x + s * 0.9, c.y + s * 0.9),
                        vec2(c.x - s * 0.9, c.y + s * 0.9),
                        cor,
                    );
                    draw_rectangle(c.x - s * 0.45, c.y - s * 0.92, s * 0.9, s * 0.16, ouro);
                }
            }
        }
        Tipo::Slot(EquipSlot::Armor) => {
            draw_rectangle(c.x - s * 0.55, c.y - s * 0.55, s * 1.1, s * 1.4, pano);
            draw_triangle(
                vec2(c.x - s * 0.55, c.y - s * 0.55),
                vec2(c.x - s * 0.95, c.y - s * 0.2),
                vec2(c.x - s * 0.55, c.y + s * 0.05),
                pano,
            );
            draw_triangle(
                vec2(c.x + s * 0.55, c.y - s * 0.55),
                vec2(c.x + s * 0.95, c.y - s * 0.2),
                vec2(c.x + s * 0.55, c.y + s * 0.05),
                pano,
            );
            draw_triangle(
                vec2(c.x - s * 0.22, c.y - s * 0.56),
                vec2(c.x + s * 0.22, c.y - s * 0.56),
                vec2(c.x, c.y - s * 0.2),
                k(0.3, 0.3, 0.34),
            );
        }
        Tipo::Slot(EquipSlot::Belt) => {
            draw_rectangle(c.x - s * 0.95, c.y - s * 0.22, s * 1.9, s * 0.44, couro);
            draw_rectangle_lines(
                c.x - s * 0.25,
                c.y - s * 0.32,
                s * 0.5,
                s * 0.64,
                s * 0.14,
                ouro,
            );
        }
        Tipo::Slot(EquipSlot::Necklace) => {
            draw_circle_lines(c.x, c.y - s * 0.25, s * 0.65, s * 0.12, ouro);
            draw_poly(c.x, c.y + s * 0.5, 4, s * 0.3, 45.0, k(0.55, 0.3, 0.9));
        }
        Tipo::Slot(EquipSlot::Earring) => {
            draw_circle_lines(c.x, c.y - s * 0.35, s * 0.3, s * 0.12, ouro);
            linha(0.0, -0.05, 0.0, 0.3, 0.1, ouro);
            draw_poly(c.x, c.y + s * 0.55, 4, s * 0.28, 45.0, k(0.35, 0.8, 0.95));
        }
        Tipo::Slot(EquipSlot::Bracelet) | Tipo::Slot(_) => {
            draw_circle_lines(c.x, c.y, s * 0.62, s * 0.26, ouro);
            draw_poly(c.x, c.y - s * 0.62, 6, s * 0.2, 0.0, k(0.9, 0.35, 0.4));
        }
        Tipo::Pocao(q) => {
            let liq = match q {
                0 => k(0.85, 0.18, 0.2),
                1 => k(0.22, 0.42, 0.95),
                // Experiencia: dourado, pra nao confundir com as de cura.
                3 => k(0.98, 0.80, 0.22),
                // Fortuna laranja, Sorte roxa: nao confundem com cura nem XP.
                4 => k(0.95, 0.55, 0.15),
                5 => k(0.70, 0.45, 0.95),
                _ => k(0.3, 0.8, 0.35),
            };
            draw_circle(c.x, c.y + s * 0.3, s * 0.62, k(0.75, 0.82, 0.88));
            draw_circle(c.x, c.y + s * 0.3, s * 0.52, liq);
            draw_rectangle(
                c.x - s * 0.18,
                c.y - s * 0.7,
                s * 0.36,
                s * 0.55,
                k(0.75, 0.82, 0.88),
            );
            draw_rectangle(c.x - s * 0.24, c.y - s * 0.9, s * 0.48, s * 0.22, couro);
            draw_circle(c.x - s * 0.2, c.y + s * 0.12, s * 0.12, k(1.0, 1.0, 1.0));
        }
        Tipo::Pergaminho => {
            crate::invocacao_ui::icone_pergaminho_de(id, c, s * 2.15);
        }
        Tipo::Ouro => {
            for (dx, dy) in [(-0.35, 0.35), (0.35, 0.35), (0.0, -0.2)] {
                draw_circle(c.x + dx * s, c.y + dy * s, s * 0.45, ouro);
                draw_circle_lines(
                    c.x + dx * s,
                    c.y + dy * s,
                    s * 0.45,
                    s * 0.08,
                    k(0.7, 0.52, 0.15),
                );
            }
        }
        Tipo::Madeira => {
            draw_rectangle(
                c.x - s * 0.8,
                c.y - s * 0.35,
                s * 1.6,
                s * 0.7,
                k(0.52, 0.34, 0.2),
            );
            draw_circle(c.x + s * 0.8, c.y, s * 0.35, k(0.78, 0.6, 0.38));
            draw_circle_lines(c.x + s * 0.8, c.y, s * 0.2, s * 0.05, k(0.52, 0.34, 0.2));
        }
        Tipo::Material | Tipo::Skin => {
            // pedra lapidada, na cor do material (sai do id: estavel entre
            // sessoes, e materiais vizinhos na tabela ficam diferentes)
            const TONS: [(f32, f32, f32); 8] = [
                (0.75, 0.78, 0.84),
                (0.46, 0.80, 0.52),
                (0.40, 0.58, 0.95),
                (0.70, 0.46, 0.95),
                (0.92, 0.70, 0.32),
                (0.88, 0.42, 0.38),
                (0.40, 0.84, 0.86),
                (0.84, 0.84, 0.50),
            ];
            let (r_, g, b) = TONS[(id as usize * 7) % TONS.len()];
            draw_poly(c.x, c.y, 6, s * 0.9, 30.0, k(r_ * 0.7, g * 0.7, b * 0.7));
            draw_poly(c.x, c.y - s * 0.1, 6, s * 0.62, 30.0, k(r_, g, b));
            draw_circle(c.x - s * 0.22, c.y - s * 0.3, s * 0.13, k(1.0, 1.0, 1.0));
        }
    }
}

#[allow(non_snake_case)]
fn VAZIA_A(a: f32) -> Color {
    com_alfa(VAZIA, a.max(0.9))
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

    #[test]
    fn desequipar_vai_pro_primeiro_espaco_vazio() {
        assert_eq!(
            primeiro_vazio(&[slot(3, 1), slot(0, 0), slot(2, 5)]),
            Some(1)
        );
        // lista mais curta que a bolsa: o proximo indice ainda e' da bolsa
        assert_eq!(primeiro_vazio(&[slot(3, 1)]), Some(1));
        let cheia: Vec<_> = (0..shared::INVENTORY_SLOTS).map(|_| slot(3, 1)).collect();
        assert_eq!(primeiro_vazio(&cheia), None);
    }

    #[test]
    fn as_abas_separam_as_categorias() {
        assert_eq!(aba_de(tipo(item_id::ESPADA_E_ESCUDO)), Aba::Equip);
        assert_eq!(aba_de(tipo(item_id::MANTO_DO_GUERREIRO)), Aba::Equip);
        assert_eq!(aba_de(tipo(item_id::HEALTH_POTION)), Aba::Consumivel);
        assert_eq!(aba_de(tipo(item_id::STEEL)), Aba::Material);
        assert_eq!(aba_de(tipo(item_id::GOLD)), Aba::Material);
        assert_eq!(
            tipo(item_id::ESPADA_E_ESCUDO),
            Tipo::Arma(Conjunto::EspadaEscudo)
        );
        assert_eq!(tipo(item_id::PISTOLAS), Tipo::Arma(Conjunto::Pistolas));
    }

    #[test]
    fn numeros_cabem_na_celula() {
        assert_eq!(curta(999), "999");
        assert_eq!(curta(5226), "5.2k");
        assert_eq!(curta(12_345), "12k");
        assert_eq!(milhar(1_234_567), "1,234,567");
        assert_eq!(milhar(12), "12");
    }

    #[test]
    fn armadura_t2_mais_cinco_tem_mais_poder_que_t1_mais_cinco() {
        let tpl = shared::items::item_template(item_id::ARMADURA_PESADA);
        let mut t1 = shared::ItemInstance::roll_em(tpl, 5, 1, 1, || 0.0).unwrap();
        let mut t2 = shared::ItemInstance::roll_em(tpl, 5, 1, 2, || 0.99).unwrap();
        t1.refinement = 5;
        t2.refinement = 5;
        assert!(poder_da_instancia(&t2) > poder_da_instancia(&t1));
    }

    #[test]
    /// O pet nao tem instancia: a cor da borda e a aba tem que sair do
    /// proprio item_id, senao todo pet apareceria cinza e como material.
    /// A borda da celula sai da cor do item. Quem guarda a cor no ID
    /// (material colorido, chave, pet) tem que devolver a cor DELE; so'
    /// equipamento rolado tira da instancia. Antes tudo sem instancia caia em
    /// cinza — um Aço Roxo aparecia igual a um Aço Cinza.
    #[test]
    fn a_borda_sai_da_cor_do_item_e_nao_so_da_instancia() {
        let peca = |id: u16| Peca {
            id,
            qty: 1,
            inst: None,
        };
        for cor in 1..=4u8 {
            assert_eq!(peca(item_id::na_cor(item_id::STEEL, cor)).grau(), cor);
            assert_eq!(peca(item_id::na_cor(item_id::ANIMA_STONE, cor)).grau(), cor);
        }
        // Chave: as quatro contiguas e a lendaria, que ficou fora da faixa.
        for cor in 1..=5u8 {
            assert_eq!(peca(item_id::chave_na_cor(item_id::HORN, cor)).grau(), cor);
        }
        assert_eq!(peca(item_id::HIDE_LENDARIA).grau(), 5);
        for grau in 1..=shared::pets::GRAU_MAX {
            assert_eq!(
                peca(item_id::pet_no_grau(item_id::PET_BASE, grau)).grau(),
                grau
            );
        }
        // Quem nao guarda cor no id continua cinza sem instancia.
        assert_eq!(peca(item_id::HEALTH_POTION).grau(), 1);
        assert_eq!(peca(item_id::GOLD).grau(), 1);
        // E equipamento rolado continua saindo da instancia.
        let mut inst = shared::items::ItemInstance::vazia_de_grau(4);
        inst.tier = 2;
        let espada = Peca {
            id: item_id::KATANA,
            qty: 1,
            inst: Some(inst),
        };
        assert_eq!(espada.grau(), 4);
        assert_eq!(espada.tier(), 2);
    }

    #[test]
    fn o_pet_pega_a_cor_do_grau_pelo_id() {
        for grau in 1..=shared::pets::GRAU_MAX {
            let id = item_id::pet_no_grau(item_id::PET_BASE, grau);
            let p = Peca {
                id,
                qty: 1,
                inst: None,
            };
            assert_eq!(p.grau(), grau, "a borda tem que ser da cor do grau");
            assert_eq!(tipo(id), Tipo::Slot(EquipSlot::Pet));
            assert_eq!(aba_de(tipo(id)), Aba::Equip);
            assert_ne!(nome_do_slot(EquipSlot::Pet), "?");
        }
        // O pergaminho continua sendo consumivel, nao equipamento.
        assert_eq!(tipo(item_id::PERGAMINHO_INVOCA_PET), Tipo::Pergaminho);
        assert_eq!(aba_de(Tipo::Pergaminho), Aba::Consumivel);
    }

    #[test]
    fn todo_slot_de_equipamento_aparece_na_tela() {
        for s in EquipSlot::TODOS {
            assert_ne!(nome_do_slot(s), "?", "{s:?} nao tem lugar na bolsa");
        }
    }
}
