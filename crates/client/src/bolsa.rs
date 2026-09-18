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

use crate::render3d;
use crate::ui;
use crate::vox::VoxCache;

const COLUNAS: usize = 8;
const VAO: f32 = 6.0;

const FUNDO: Color = Color::new(0.06, 0.06, 0.08, 0.97);
const SECAO: Color = Color::new(0.10, 0.09, 0.12, 1.0);
const VAZIA: Color = Color::new(0.13, 0.12, 0.15, 1.0);
const BORDA: Color = Color::new(0.27, 0.23, 0.19, 1.0);
const TEXTO: Color = Color::new(0.88, 0.87, 0.85, 1.0);
const APAGADO: Color = Color::new(0.52, 0.51, 0.54, 1.0);
const VERDE: Color = Color::new(0.45, 0.80, 0.42, 1.0);
const VERMELHO: Color = Color::new(0.88, 0.38, 0.32, 1.0);

/// Os slots em volta do retrato, como no MIR4: o que se empunha e se veste
/// de um lado, os acessorios do outro (docs/COMBATE.md).
const ESQUERDA: [(EquipSlot, &str); 3] = [
    (EquipSlot::Weapon, "Arma"),
    (EquipSlot::Offhand, "Secundária"),
    (EquipSlot::Armor, "Armadura"),
];
const DIREITA: [(EquipSlot, &str); 4] = [
    (EquipSlot::Earring, "Brinco"),
    (EquipSlot::Necklace, "Amuleto"),
    (EquipSlot::Bracelet, "Bracelete"),
    (EquipSlot::Belt, "Cinto"),
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
    (Aba::Tudo, "Tudo"),
    (Aba::Equip, "Equipamento"),
    (Aba::Consumivel, "Consumível"),
    (Aba::Material, "Material"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Acao {
    Equipar(usize),
    Usar(usize),
    Desequipar(EquipSlot),
    Organizar,
}

/// O que o jogador tem, como o servidor contou, e o estado da tela.
pub struct Bolsa {
    pub aberta: bool,
    pub slots: Vec<InventorySlot>,
    pub equip: shared::Equipment,
    pub stats: Option<PlayerStats>,
    /// Nome de cada item (`ServerMessage::ItemsConfig`).
    pub nomes: HashMap<u16, String>,
    pub ouro: u64,
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
}

impl Default for Bolsa {
    fn default() -> Self {
        Bolsa {
            aberta: false,
            slots: Vec::new(),
            equip: Default::default(),
            stats: None,
            nomes: HashMap::new(),
            ouro: 0,
            nivel: 0,
            aba: Aba::Tudo,
            sel: None,
            clique: (0.0, None),
            aviso: None,
            onde_obter: None,
            refinar: None,
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
        _ => Tipo::Material,
    }
}

fn aba_de(t: Tipo) -> Aba {
    match t {
        Tipo::Arma(_) | Tipo::Slot(_) => Aba::Equip,
        Tipo::Pocao(_) => Aba::Consumivel,
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
fn curta(q: u32) -> String {
    if q >= 10_000 {
        format!("{}k", q / 1000)
    } else if q >= 1000 {
        format!("{:.1}k", q as f32 / 1000.0).replace('.', ",")
    } else {
        q.to_string()
    }
}

/// 1234567 -> "1.234.567".
pub(crate) fn milhar(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Primeiro espaco livre da bolsa, contando os que o servidor ainda nem
/// mandou (a lista pode vir mais curta que a bolsa).
fn primeiro_vazio(slots: &[InventorySlot]) -> Option<usize> {
    slots
        .iter()
        .position(|s| s.qty == 0)
        .or((slots.len() < shared::INVENTORY_SLOTS).then_some(slots.len()))
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
    p.inst.map_or(0, |i| poder_da_instancia(&i))
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

fn tela() -> Tela {
    let (sw, sh) = (screen_width(), screen_height());
    let w = (sw - 40.0).min(1080.0);
    let h = (sh - 40.0).min(680.0);
    let painel = Rect::new((sw - w) * 0.5, (sh - h) * 0.5, w, h);
    let esq_w = (w * 0.44).floor();
    let esq = Rect::new(painel.x + 16.0, painel.y + 56.0, esq_w, h - 72.0);
    let dx = esq.x + esq.w + 16.0;
    let dir = Rect::new(dx, esq.y, painel.x + w - 16.0 - dx, esq.h);
    let cel = ((dir.w - (COLUNAS as f32 - 1.0) * VAO) / COLUNAS as f32)
        .min(62.0)
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
    r.contains(mouse()) && is_mouse_button_pressed(MouseButton::Left)
}

impl Bolsa {
    /// Abre pelo icone do HUD ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberta = true;
        self.sel = None;
    }

    pub fn alterna(&mut self) {
        self.aberta = !self.aberta;
        self.sel = None;
    }

    pub fn fecha(&mut self) {
        self.aberta = false;
        self.sel = None;
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
                Tipo::Pocao(_) => Some(Acao::Usar(i)),
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
            Acao::Desequipar(s) => match primeiro_vazio(&self.slots) {
                Some(i) => Some(ClientMessage::InventorySwap {
                    a: InvSpot::Equip(s),
                    b: InvSpot::Inv(i as u16),
                }),
                None => {
                    self.aviso = Some(("A bolsa está cheia.".into(), get_time()));
                    None
                }
            },
            Acao::Organizar => Some(ClientMessage::InventoryAutoArrange),
        }
    }

    /// Desenha a bolsa aberta. Devolve o pedido pro servidor, se o jogador fez
    /// alguma coisa.
    pub fn desenha(&mut self, vox: &VoxCache, solido: &Material) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
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
        crate::hud_estilo::separador(p.x + 16.0, p.y + 46.0, p.w - 32.0);
        ui::texto(p.x + 22.0, p.y + 33.0, "BOLSA", 26, ui::OURO);
        let ouro = format!("Ouro  {}", milhar(self.ouro));
        let d = crate::hud_estilo::medir_dim(&ouro, 20);
        ui::texto(
            p.x + p.w - 70.0 - d.width,
            p.y + 31.0,
            &ouro,
            20,
            ui::OURO_CLARO,
        );
        draw_circle(p.x + p.w - 84.0 - d.width, p.y + 25.0, 7.0, ui::OURO);
        if ui::botao(
            Rect::new(p.x + p.w - 50.0, p.y + 9.0, 34.0, 30.0),
            "x",
            true,
        ) {
            self.fecha();
            return None;
        }

        let cartao = Rect::new(
            t.esq.x + 18.0,
            t.esq.y + 44.0,
            t.esq.w - 36.0,
            (t.esq.h - 60.0).min(380.0),
        );
        let bloqueio = self.sel.map(|_| cartao);
        let mut acao = self.desenha_equipamento(t.esq, vox, solido, bloqueio);
        if let Some(a) = self.desenha_grade(t.dir, t.cel) {
            acao = Some(a);
        }
        if let Some(sel) = self.sel {
            match self.peca(sel) {
                Some(peca) => {
                    if let Some(a) = self.desenha_cartao(cartao, sel, peca) {
                        acao = Some(a);
                    }
                }
                None => self.sel = None,
            }
        }
        if let Some((msg, quando)) = &self.aviso {
            if get_time() - quando < 2.5 {
                ui::texto_centro(p.x + p.w * 0.5, p.y + p.h - 10.0, msg, 18, VERMELHO);
            } else {
                self.aviso = None;
            }
        }
        acao.and_then(|a| self.pedido(a))
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
        ui::texto(r.x + 14.0, r.y + 24.0, "Equipamento", 20, ui::OURO);
        if self.nivel > 0 {
            let n = format!("Nível {}", self.nivel);
            let d = crate::hud_estilo::medir_dim(&n, 17);
            ui::texto(r.x + r.w - 14.0 - d.width, r.y + 24.0, &n, 17, TEXTO);
        }
        // O que esta' na mao — a pergunta que a bolsa existe pra responder.
        let arma = self.equip.weapon;
        let em_uso = match arma {
            Some(id) => format!(
                "Em uso: {} · {}",
                Conjunto::da_arma(id).nome(),
                self.nome(id)
            ),
            None => "Em uso: nenhuma arma".to_string(),
        };
        ui::texto(r.x + 14.0, r.y + 44.0, &em_uso, 16, ui::OURO_CLARO);

        let s = ((r.h - 60.0 - 170.0) / 4.0 - 16.0)
            .clamp(40.0, 64.0)
            .floor();
        let passo = s + 16.0;
        let y0 = r.y + 60.0;
        let xe = r.x + 14.0;
        let xd = r.x + r.w - 14.0 - s;
        let retrato = Rect::new(
            xe + s + 12.0,
            y0,
            xd - 12.0 - (xe + s + 12.0),
            4.0 * passo - 16.0,
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
                celula(c, peca, sel, Some(*slot));
                ui::texto_centro(c.x + s * 0.5, c.y + s + 12.0, rotulo, 12, APAGADO);
                let livre = bloqueio.map_or(true, |b| !b.contains(mouse()));
                if livre && peca.is_some() && clicou_em(c) {
                    clicado = Some(Sel::Equip(*slot));
                }
            }
        }
        if let Some(sel) = clicado {
            acao = self.clica(sel);
        }

        // O poder e a ficha, embaixo do retrato.
        let mut y = y0 + 4.0 * passo + 8.0;
        if let Some(st) = &self.stats {
            ui::texto_centro(r.x + r.w * 0.5, y + 12.0, "PODER", 14, APAGADO);
            ui::texto_centro(
                r.x + r.w * 0.5,
                y + 42.0,
                &milhar(poder(st).max(0) as u64),
                32,
                ui::OURO,
            );
            y += 62.0;
            let linhas = [
                ("Ataque", st.attack_damage.to_string()),
                ("Defesa", st.defense.to_string()),
                ("Vida", st.hp_max.to_string()),
                ("Mana", st.mp_max.to_string()),
                ("Destreza", st.dex.to_string()),
                ("Sabedoria", st.wis.to_string()),
                ("Crítico", format!("{:.1}%", st.crit_chance * 100.0)),
                (
                    "Vel. ataque",
                    format!("{:.0}%", st.attack_speed_mult * 100.0),
                ),
            ];
            let col_w = (r.w - 28.0) * 0.5;
            for (i, (rot, val)) in linhas.iter().enumerate() {
                let cx = r.x + 14.0 + (i % 2) as f32 * col_w;
                let cy = y + (i / 2) as f32 * 20.0;
                if cy > r.y + r.h - 6.0 {
                    break;
                }
                ui::texto(cx, cy, rot, 16, APAGADO);
                let d = crate::hud_estilo::medir_dim(val, 16);
                ui::texto(cx + col_w - 16.0 - d.width, cy, val, 16, TEXTO);
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
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.09, 0.08, 0.11, 1.0));
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
                "Retrato sem espaço na tela",
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
        render3d::desenha_rig(base, &pose, corpo, vox.rig(render3d::RIG_CHAPEU), vox, None);
        gl_use_default_material();
        set_default_camera();
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

    fn desenha_grade(&mut self, r: Rect, cel: f32) -> Option<Acao> {
        let mut acao = None;
        // abas
        let aba_w = (r.w - 3.0 * 6.0) / 4.0;
        for (k, (aba, rotulo)) in ABAS.iter().enumerate() {
            let a = Rect::new(r.x + k as f32 * (aba_w + 6.0), r.y, aba_w, 32.0);
            let ativa = self.aba == *aba;
            let sobre = a.contains(mouse());
            crate::hud_estilo::aba(a, rotulo, ativa, sobre);
            if clicou_em(a) {
                self.aba = *aba;
                self.sel = None;
            }
        }

        // Quais espacos aparecem: em "Tudo", a bolsa inteira na ordem dela; nas
        // outras abas, so' os itens daquela categoria, juntos no comeco.
        let mut mostrar: Vec<Option<usize>> = if self.aba == Aba::Tudo {
            (0..shared::INVENTORY_SLOTS).map(Some).collect()
        } else {
            (0..self.slots.len())
                .filter(|&i| {
                    self.slots[i].qty > 0 && aba_de(tipo(self.slots[i].item_id)) == self.aba
                })
                .map(Some)
                .collect()
        };
        let linhas = shared::INVENTORY_SLOTS.div_ceil(COLUNAS);
        mostrar.resize(mostrar.len().max(linhas * COLUNAS), None);

        let y0 = r.y + 44.0;
        let mut clicado = None;
        for (k, onde) in mostrar.iter().enumerate() {
            let (col, lin) = (k % COLUNAS, k / COLUNAS);
            let c = Rect::new(
                r.x + col as f32 * (cel + VAO),
                y0 + lin as f32 * (cel + VAO),
                cel,
                cel,
            );
            if c.y + c.h > r.y + r.h - 50.0 {
                break;
            }
            let peca = onde.and_then(|i| self.peca(Sel::Inv(i)));
            let sel = onde.is_some() && self.sel == onde.map(Sel::Inv);
            celula(c, peca, sel, None);
            if let (Some(i), Some(_)) = (onde, peca) {
                if clicou_em(c) {
                    clicado = Some(Sel::Inv(*i));
                }
            }
        }
        if let Some(s) = clicado {
            acao = self.clica(s);
        }

        // o pe': ocupacao, dica e organizar
        let ocupados = self.slots.iter().filter(|s| s.qty > 0).count();
        let pe = r.y + r.h - 40.0;
        ui::texto(
            r.x,
            pe + 22.0,
            &format!("{ocupados}/{}", shared::INVENTORY_SLOTS),
            20,
            if ocupados >= shared::INVENTORY_SLOTS {
                VERMELHO
            } else {
                TEXTO
            },
        );
        ui::texto(
            r.x + 70.0,
            pe + 21.0,
            "dois cliques: equipar ou usar",
            15,
            APAGADO,
        );
        if ui::botao(
            Rect::new(r.x + r.w - 130.0, pe, 130.0, 34.0),
            "Organizar",
            true,
        ) {
            acao = Some(Acao::Organizar);
        }
        acao
    }

    // ── o cartao do item ──

    fn desenha_cartao(&mut self, r: Rect, sel: Sel, peca: Peca) -> Option<Acao> {
        let cor = cor_do_tier(peca.tier());
        crate::hud_estilo::ret_arredondado(
            r,
            crate::hud_estilo::RAIO,
            Color::new(0.05, 0.06, 0.09, 0.97),
        );
        crate::hud_estilo::painel_destaque(r, cor);
        crate::hud_estilo::borda_arredondada(r, crate::hud_estilo::RAIO, 1.0, com_alfa(cor, 0.55));

        let ic = Rect::new(r.x + 16.0, r.y + 18.0, 64.0, 64.0);
        celula(ic, Some(peca), false, None);
        let tx = ic.x + ic.w + 14.0;
        let nome = self.nome(peca.id);
        let titulo = if peca.refino() > 0 {
            format!("+{} {nome}", peca.refino())
        } else {
            nome
        };
        ui::texto(tx, r.y + 40.0, &titulo, 22, cor);
        if !matches!(tipo(peca.id), Tipo::Ouro)
            && crate::onde_obter::botao(Rect::new(r.x + r.w - 52.0, r.y + 14.0, 38.0, 38.0))
        {
            self.onde_obter = Some(peca.id);
        }
        let t = tipo(peca.id);
        let classe = match t {
            Tipo::Arma(c) => format!("Arma · {}", c.nome()),
            Tipo::Slot(s) => nome_do_slot(s).to_string(),
            Tipo::Pocao(_) => "Consumível".into(),
            Tipo::Ouro => "Moeda".into(),
            _ => "Material".into(),
        };
        ui::texto(tx, r.y + 62.0, &classe, 16, TEXTO);
        if let Some(i) = peca.inst {
            ui::texto(
                tx,
                r.y + 80.0,
                &format!(
                    "Tier {} · nível do item {}",
                    ROMANO[(i.tier() - 1) as usize],
                    i.item_level
                ),
                15,
                APAGADO,
            );
        } else if peca.qty > 1 {
            ui::texto(
                tx,
                r.y + 80.0,
                &format!("Quantidade {}", milhar(peca.qty as u64)),
                15,
                APAGADO,
            );
        }

        let mut y = r.y + 112.0;
        crate::hud_estilo::separador(r.x + 16.0, y - 12.0, r.w - 32.0);
        if let Some(i) = peca.inst {
            let atributos = [
                ("Ataque", i.attack_damage),
                ("Defesa", i.defense),
                ("Vida", i.hp_max),
                ("Mana", i.mp_max),
                ("Destreza", i.dex),
                ("Sabedoria", i.wis),
            ];
            for (rot, v) in atributos.iter().filter(|(_, v)| *v != 0) {
                ui::texto(r.x + 20.0, y + 4.0, rot, 17, TEXTO);
                let val = format!("+{v}");
                let d = crate::hud_estilo::medir_dim(&val, 17);
                ui::texto(r.x + r.w - 20.0 - d.width, y + 4.0, &val, 17, VERDE);
                y += 22.0;
            }
            if let Some(req) = i.level_req {
                let falta = (self.nivel as u16) < req && self.nivel > 0;
                ui::texto(
                    r.x + 20.0,
                    y + 8.0,
                    &format!("Requer nível {req}"),
                    16,
                    if falta { VERMELHO } else { APAGADO },
                );
                y += 24.0;
            }
            // Comparacao com o que esta' vestido naquele slot — o MIR4 mostra a
            // seta; aqui vai o saldo de poder.
            if let (Sel::Inv(_), Some(slot)) = (sel, shared::equip_slot_of(peca.id)) {
                let vestido = self.peca(Sel::Equip(slot));
                let saldo = poder_da_peca(&peca) - vestido.map_or(0, |v| poder_da_peca(&v));
                let (txt, c) = match saldo {
                    0 => ("mesmo poder do vestido".to_string(), APAGADO),
                    s if s > 0 => (format!("+{s} de poder sobre o vestido"), VERDE),
                    s => (format!("{s} de poder sobre o vestido"), VERMELHO),
                };
                ui::texto(r.x + 20.0, y + 10.0, &txt, 16, c);
            }
        } else {
            let txt = match t {
                Tipo::Pocao(0) => "Recupera vida.",
                Tipo::Pocao(1) => "Recupera mana.",
                Tipo::Pocao(3) => "+30% de XP por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(4) => "+30% de ouro e cobre dos bichos por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(5) => "+20% de chance de drop (bichos e coleta) por 1 hora. Beber outra renova a hora.",
                Tipo::Pocao(_) => "Recupera vigor.",
                Tipo::Arma(_) | Tipo::Slot(_) => "Peça básica, sem atributos rolados.",
                _ => "Material de criação.",
            };
            ui::texto(r.x + 20.0, y + 4.0, txt, 16, APAGADO);
        }

        // acoes
        let by = r.y + r.h - 50.0;
        let principal = match sel {
            Sel::Equip(s) => Some(("Desequipar", Acao::Desequipar(s))),
            Sel::Inv(i) => match t {
                Tipo::Arma(_) | Tipo::Slot(_) => Some(("Equipar", Acao::Equipar(i))),
                Tipo::Pocao(_) => Some(("Usar", Acao::Usar(i))),
                _ => None,
            },
        };
        let mut acao = None;
        // Uma fileira so': [acao principal] [Refinar +N] [Fechar]. No celular
        // o cartao e' baixo e uma segunda fileira cobriria os atributos.
        // Refinar so' em peca com atributos rolados, e leva pra Forja com ela
        // ja' escolhida.
        let bw3 = (r.w - 32.0 - 16.0) / 3.0;
        let coluna = |k: f32| Rect::new(r.x + 16.0 + k * (bw3 + 8.0), by, bw3, 36.0);
        if let Some((rot, a)) = principal {
            if ui::botao(coluna(0.0), rot, true) {
                acao = Some(a);
            }
        }
        if let Some(i) = peca.inst {
            let (rot, pode) = if i.refinement >= shared::forja::REFINO_MAX {
                ("Refino máx.".to_string(), false)
            } else {
                (format!("Refinar +{}", i.refinement + 1), true)
            };
            if ui::botao(coluna(1.0), &rot, pode) {
                self.refinar = Some(match sel {
                    Sel::Inv(i) => shared::protocol::AlvoDaForja::Bolsa(i as u16),
                    Sel::Equip(s) => shared::protocol::AlvoDaForja::Equipado(s),
                });
                self.sel = None;
            }
        }
        if ui::botao(coluna(2.0), "Fechar", true) {
            self.sel = None;
        }
        acao
    }
}

/// Uma celula de item: fundo na cor do grau, o icone, o tier em romano no
/// canto de cima, o refino do outro lado e a quantidade embaixo. `vazio` e' o
/// slot de equipamento sem nada: aparece a silhueta apagada do que vai ali.
fn celula(r: Rect, peca: Option<Peca>, selecionada: bool, vazio: Option<EquipSlot>) {
    let sobre = r.contains(mouse());
    match peca {
        Some(p) => {
            let cor = cor_do_tier(p.tier());
            crate::hud_estilo::slot(r, Some(cor), sobre, false);
            icone_do_item(r, p.id, 1.0);
            let fonte = (r.w * 0.26).clamp(11.0, 16.0) as u16;
            if p.inst.is_some() {
                ui::texto(
                    r.x + 4.0,
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
                    r.x + r.w - d.width - 4.0,
                    r.y + fonte as f32,
                    &t,
                    fonte,
                    ui::OURO_CLARO,
                );
            }
            if p.qty > 1 {
                let t = curta(p.qty);
                let d = crate::hud_estilo::medir_dim(&t, fonte);
                ui::texto(r.x + r.w - d.width - 3.0, r.y + r.h - 4.0, &t, fonte, BLACK);
                ui::texto(r.x + r.w - d.width - 4.0, r.y + r.h - 5.0, &t, fonte, TEXTO);
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
            Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
            crate::hud_estilo::RAIO_PEQUENO + 4.0,
            2.0,
            ui::OURO,
        );
    }
}

/// O icone de um item (bolsa, loja, barra, craft, forja). A arte e' o atlas
/// de `icones`; o desenho por categoria abaixo so' cobre item sem icone.
pub(crate) fn icone_do_item(r: Rect, id: u16, a: f32) {
    if crate::icones::desenha(id, r, a) {
        return;
    }
    icone(r, tipo(id), id, a);
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
        Tipo::Material => {
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
        assert_eq!(curta(5226), "5,2k");
        assert_eq!(curta(12_345), "12k");
        assert_eq!(milhar(1_234_567), "1.234.567");
        assert_eq!(milhar(12), "12");
    }

    #[test]
    fn todo_slot_de_equipamento_aparece_na_tela() {
        for s in EquipSlot::TODOS {
            assert_ne!(nome_do_slot(s), "?", "{s:?} nao tem lugar na bolsa");
        }
    }
}
