//! The item bar (MIR4): four configurable slots — C, 8, 9 and 0 — each with
//! the consumable it uses, whether it uses it on its own (AUTO) and the threshold.
//!
//! The same gesture as the skills: a short click USES, dragging up switches
//! that slot's AUTO on, down switches it off. The CLIENT decides WHEN to
//! drink; the server validates the `UseItem` as always. The configuration
//! lives on the character, on the server (`SalvarBarra` / `BarraDeItens`).
use macroquad::prelude::{Rect, Vec2};
use shared::constants::item_id as it;
use shared::protocol::{EspacoDaBarra, ESPACOS_DA_BARRA};
use shared::InventorySlot;

use crate::habilidades_input::{Arrasto, Gesto};

pub const ESPACOS: usize = ESPACOS_DA_BARRA;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Categoria {
    Vida,
    Mana,
    Vigor,
    Experiencia,
    Fortuna,
    Sorte,
}

/// What an item is on the bar. `None` = it does not go on the bar.
pub fn categoria(id: u16) -> Option<Categoria> {
    match id {
        x if x == it::HEALTH_POTION || x == it::GREATER_HEAL => Some(Categoria::Vida),
        x if x == it::MANA_POTION || x == it::GREATER_MANA => Some(Categoria::Mana),
        x if x == it::STAMINA_POTION => Some(Categoria::Vigor),
        x if x == it::XP_POTION => Some(Categoria::Experiencia),
        x if x == it::FORTUNA_POTION => Some(Categoria::Fortuna),
        x if x == it::SORTE_POTION => Some(Categoria::Sorte),
        _ => None,
    }
}

impl Categoria {
    /// AUTO threshold in % for a resource; a buff has none (it uses when it runs out).
    pub fn limiar_padrao(self) -> Option<u8> {
        match self {
            Categoria::Vida => Some(60),
            Categoria::Mana => Some(40),
            Categoria::Vigor => Some(30),
            _ => None,
        }
    }

    /// Cooldown group (`shared::pocoes`); a buff has none.
    pub fn grupo(self) -> Option<shared::pocoes::Grupo> {
        match self {
            Categoria::Vida => Some(shared::pocoes::Grupo::Vida),
            Categoria::Mana => Some(shared::pocoes::Grupo::Mana),
            Categoria::Vigor => Some(shared::pocoes::Grupo::Vigor),
            _ => None,
        }
    }

    /// Minimum interval between two requests. What holds a resource potion back
    /// is the group's cooldown (the server refuses and sends `PocaoGrupo`); this
    /// one only avoids asking again before the answer arrives. A buff waits
    ///  longer — the "active" comes in a message of its own.
    fn intervalo_s(self) -> f64 {
        match self {
            Categoria::Vida | Categoria::Mana | Categoria::Vigor => 1.0,
            _ => 10.0,
        }
    }

    /// The family: Health and Health+ count together and one covers the other.
    fn familia(self) -> &'static [u16] {
        match self {
            Categoria::Vida => &[it::HEALTH_POTION, it::GREATER_HEAL],
            Categoria::Mana => &[it::MANA_POTION, it::GREATER_MANA],
            Categoria::Vigor => &[it::STAMINA_POTION],
            Categoria::Experiencia => &[it::XP_POTION],
            Categoria::Fortuna => &[it::FORTUNA_POTION],
            Categoria::Sorte => &[it::SORTE_POTION],
        }
    }

    /// How AUTO fires, for the configurator's text.
    pub fn regra(self) -> &'static str {
        match self {
            Categoria::Vida => "health below the threshold",
            Categoria::Mana => "mana below the threshold",
            Categoria::Vigor => "stamina below the threshold",
            Categoria::Experiencia => "no XP bonus active",
            Categoria::Fortuna => "no Fortune active",
            Categoria::Sorte => "no Luck active",
        }
    }
}

/// A slot with the item and its default threshold, AUTO off.
pub fn espaco(id: u16, auto: bool) -> EspacoDaBarra {
    EspacoDaBarra {
        item_id: id,
        auto,
        limiar: categoria(id).and_then(|c| c.limiar_padrao()).unwrap_or(0),
    }
}

/// The bar of someone who never configured it: health with AUTO (as in
/// MIR4), mana, stamina and experience manual.
pub fn padrao() -> [EspacoDaBarra; ESPACOS] {
    [
        espaco(it::HEALTH_POTION, true),
        espaco(it::MANA_POTION, false),
        espaco(it::STAMINA_POTION, false),
        espaco(it::XP_POTION, false),
    ]
}

/// How much of the slot's consumable the bag holds (the whole family).
pub fn quantidade(slots: &[InventorySlot], id: u16) -> u32 {
    let Some(c) = categoria(id) else { return 0 };
    slots
        .iter()
        .filter(|s| c.familia().contains(&s.item_id) && s.instance.is_none())
        .map(|s| s.qty)
        .sum()
}

/// The bag slot to use. `forte`: the largest of the family first; otherwise
/// the chosen item and, failing that, any other of the family.
pub fn slot_para_usar(slots: &[InventorySlot], id: u16, forte: bool) -> Option<usize> {
    let c = categoria(id)?;
    let tem = |x: u16| {
        slots
            .iter()
            .position(|s| s.item_id == x && s.qty > 0 && s.instance.is_none())
    };
    if forte {
        if let Some(i) = c.familia().last().and_then(|&maior| tem(maior)) {
            return Some(i);
        }
    }
    tem(id).or_else(|| c.familia().iter().find_map(|&x| tem(x)))
}

/// What AUTO needs to know about the character. Fractions from 0 to 1.
#[derive(Clone, Copy, Debug)]
pub struct Estado {
    pub vivo: bool,
    pub hp: f32,
    pub mp: f32,
    pub vigor: f32,
    pub xp_ativo: bool,
    pub fortuna_ativo: bool,
    pub sorte_ativo: bool,
}

pub struct Barra {
    pub espacos: [EspacoDaBarra; ESPACOS],
    ultimo: [f64; ESPACOS],
    arrasto: Arrasto,
    /// End of the cooldown and of the heal-over-time of each potion group (the
    /// client's clock), coming from `PocaoGrupo`.
    recarga_ate: [f64; shared::pocoes::GRUPOS],
    cura_ate: [f64; shared::pocoes::GRUPOS],
}

impl Default for Barra {
    fn default() -> Self {
        Self {
            espacos: padrao(),
            ultimo: [f64::MIN; ESPACOS],
            arrasto: Arrasto::default(),
            recarga_ate: [f64::MIN; shared::pocoes::GRUPOS],
            cura_ate: [f64::MIN; shared::pocoes::GRUPOS],
        }
    }
}

impl Barra {
    /// `PocaoGrupo` do servidor: recarga e cura restantes do grupo.
    pub fn pocao_grupo(&mut self, grupo: u8, recarga_s: f32, cura_s: f32, agora: f64) {
        if let Some(g) = shared::pocoes::Grupo::de_u8(grupo) {
            self.recarga_ate[g as usize] = agora + recarga_s.max(0.0) as f64;
            self.cura_ate[g as usize] = agora + cura_s.max(0.0) as f64;
        }
    }

    /// (restante, total) da recarga do espaco, se estiver em recarga.
    pub fn recarga(&self, i: usize, agora: f64) -> Option<(f32, f32)> {
        let e = self.espacos.get(i)?;
        let g = categoria(e.item_id)?.grupo()?;
        let resta = (self.recarga_ate[g as usize] - agora) as f32;
        if resta <= 0.0 {
            return None;
        }
        let total = shared::pocoes::cura_de(e.item_id)
            .map_or(resta, |c| c.recarga_s as f32)
            .max(resta);
        Some((resta, total))
    }

    /// Is the slot's group heal-over-time running?
    pub fn curando(&self, i: usize, agora: f64) -> bool {
        self.espacos
            .get(i)
            .and_then(|e| categoria(e.item_id))
            .and_then(|c| c.grupo())
            .is_some_and(|g| self.cura_ate[g as usize] > agora)
    }

    /// Seconds of heal left per group (health, mana, stamina), for the HUD.
    pub fn curas(&self, agora: f64) -> [f32; shared::pocoes::GRUPOS] {
        std::array::from_fn(|g| (self.cura_ate[g] - agora).max(0.0) as f32)
    }

    /// What the server stored. Empty (never configured) falls back to the default.
    pub fn do_servidor(&mut self, v: &[EspacoDaBarra]) {
        self.espacos = padrao();
        if v.is_empty() {
            return;
        }
        self.espacos = [EspacoDaBarra::default(); ESPACOS];
        for (i, e) in v.iter().take(ESPACOS).enumerate() {
            self.espacos[i] = *e;
        }
    }

    pub fn para_servidor(&self) -> Vec<EspacoDaBarra> {
        self.espacos.to_vec()
    }

    pub fn atribuir(&mut self, i: usize, id: u16) {
        if let Some(e) = self.espacos.get_mut(i) {
            *e = espaco(id, false);
        }
    }

    pub fn limpar(&mut self, i: usize) {
        if let Some(e) = self.espacos.get_mut(i) {
            *e = EspacoDaBarra::default();
        }
    }

    pub fn alterna_auto(&mut self, i: usize) {
        if let Some(e) = self.espacos.get_mut(i).filter(|e| e.item_id != 0) {
            e.auto = !e.auto;
        }
    }

    /// Threshold in steps of 5, between 5% and 95%.
    pub fn ajustar_limiar(&mut self, i: usize, delta: i32) {
        if let Some(e) = self.espacos.get_mut(i) {
            e.limiar = (e.limiar as i32 + delta).clamp(5, 95) as u8;
        }
    }

    /// This frame's mouse over the buttons. Returns the completed gesture.
    pub fn entrada(
        &mut self,
        rects: &[Rect; ESPACOS],
        mouse: Vec2,
        apertou: bool,
        segurando: bool,
        soltou: bool,
    ) -> Option<Gesto> {
        if apertou && self.arrasto.inicio.is_none() {
            if let Some(i) = rects.iter().position(|r| r.contains(mouse)) {
                self.arrasto.pressiona(i as u32, mouse);
            }
        }
        self.arrasto.move_para(mouse);
        let (id, _) = self.arrasto.inicio?;
        if soltou {
            return self.arrasto.solta(mouse, rects[id as usize]);
        }
        if !segurando {
            self.arrasto.cancela();
        }
        None
    }

    /// Forgets the gesture in progress (the long press became "open the
    /// configurator": releasing afterwards neither uses the item nor touches AUTO).
    pub fn cancela_gesto(&mut self) {
        self.arrasto.cancela();
    }

    /// Dragging: which slot and where it came from (for the "^ Release: AUTO" hint).
    pub fn arrastando(&self) -> Option<(usize, Vec2)> {
        self.arrasto.inicio.map(|(i, p)| (i as usize, p))
    }

    /// Aplica um gesto: `(espaco a usar, mudou a configuracao)`.
    pub fn aplica(&mut self, gesto: Gesto) -> (Option<usize>, bool) {
        match gesto {
            Gesto::Usar(i) => (Some(i as usize), false),
            Gesto::Auto(i, ligar) => {
                match self.espacos.get_mut(i as usize).filter(|e| e.item_id != 0) {
                    Some(e) if e.auto != ligar => {
                        e.auto = ligar;
                        (None, true)
                    }
                    _ => (None, false),
                }
            }
        }
    }

    /// Which slot to use now, if any: `(index, strong)`.
    ///
    /// The AUTO rule for resource potions: below the threshold, with the group
    /// OUT of cooldown (inside it the server would refuse — and the cooldown
    /// covers the whole heal, so there is never a heal running outside the
    /// cooldown). The tier is the SMALLEST whose total covers what is missing to
    /// the maximum; if none covers it, the largest there is
    /// (`shared::pocoes::escolher`). `forte` = use the largest of the family.
    pub fn decide(
        &mut self,
        e: &Estado,
        qtd: &[u32; ESPACOS],
        slots: &[InventorySlot],
        agora: f64,
    ) -> Option<(usize, bool)> {
        if !e.vivo {
            return None;
        }
        for i in 0..ESPACOS {
            let esp = self.espacos[i];
            if !esp.auto || qtd[i] == 0 {
                continue;
            }
            let Some(c) = categoria(esp.item_id) else {
                continue;
            };
            if c.grupo()
                .is_some_and(|g| agora < self.recarga_ate[g as usize])
            {
                continue;
            }
            let limiar = esp.limiar as f32 / 100.0;
            let fracao = match c {
                Categoria::Vida => e.hp,
                Categoria::Mana => e.mp,
                Categoria::Vigor => e.vigor,
                _ => 1.0,
            };
            let precisa = match c {
                Categoria::Vida | Categoria::Mana | Categoria::Vigor => fracao < limiar,
                Categoria::Experiencia => !e.xp_ativo,
                Categoria::Fortuna => !e.fortuna_ativo,
                Categoria::Sorte => !e.sorte_ativo,
            };
            if precisa && agora - self.ultimo[i] >= c.intervalo_s() {
                self.ultimo[i] = agora;
                let familia: Vec<(u16, u32)> = c
                    .familia()
                    .iter()
                    .map(|&id| {
                        (
                            id,
                            slots
                                .iter()
                                .filter(|s| s.item_id == id && s.instance.is_none())
                                .map(|s| s.qty)
                                .sum(),
                        )
                    })
                    .collect();
                let forte = c.grupo().is_some()
                    && c.familia().len() > 1
                    && shared::pocoes::escolher(&familia, 1.0 - fracao)
                        == c.familia().last().copied();
                return Some((i, forte));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::vec2;

    fn cheio() -> Estado {
        Estado {
            vivo: true,
            hp: 1.0,
            mp: 1.0,
            vigor: 1.0,
            xp_ativo: true,
            fortuna_ativo: true,
            sorte_ativo: true,
        }
    }

    fn slot(id: u16, qty: u32) -> InventorySlot {
        InventorySlot {
            item_id: id,
            qty,
            instance: None,
        }
    }

    #[test]
    fn clique_usa_e_arrastar_pra_cima_liga_o_auto_do_espaco() {
        let rects = [
            Rect::new(0.0, 100.0, 60.0, 60.0),
            Rect::new(70.0, 100.0, 40.0, 40.0),
            Rect::new(120.0, 100.0, 40.0, 40.0),
            Rect::new(170.0, 100.0, 40.0, 40.0),
        ];
        let mut b = Barra::default();
        b.entrada(&rects, vec2(90.0, 120.0), true, true, false);
        let g = b
            .entrada(&rects, vec2(90.0, 120.0), false, false, true)
            .unwrap();
        assert_eq!(b.aplica(g), (Some(1), false), "clique curto usa");
        b.entrada(&rects, vec2(90.0, 120.0), true, true, false);
        b.entrada(&rects, vec2(90.0, 70.0), false, true, false);
        assert!(b.arrastando().is_some());
        let g = b
            .entrada(&rects, vec2(90.0, 60.0), false, false, true)
            .unwrap();
        assert_eq!(
            b.aplica(g),
            (None, true),
            "arrastar pra cima muda a configuracao"
        );
        assert!(b.espacos[1].auto);
        b.entrada(&rects, vec2(90.0, 120.0), true, true, false);
        let g = b
            .entrada(&rects, vec2(90.0, 170.0), false, false, true)
            .unwrap();
        b.aplica(g);
        assert!(!b.espacos[1].auto, "pra baixo desliga");
        // An empty slot does not switch AUTO on.
        b.limpar(2);
        assert_eq!(b.aplica(Gesto::Auto(2, true)), (None, false));
    }

    #[test]
    fn auto_respeita_limiar_recarga_do_grupo_e_escolhe_o_tier_pelo_deficit() {
        let mut b = Barra::default();
        let qtd = [5; ESPACOS];
        let slots = vec![slot(it::HEALTH_POTION, 5), slot(it::GREATER_HEAL, 5)];
        let mut e = cheio();
        assert_eq!(b.decide(&e, &qtd, &slots, 10.0), None, "cheio nao gasta");
        // Falta 50%: nenhuma cobre (14% / 21%) — a maior.
        e.hp = 0.5;
        assert_eq!(b.decide(&e, &qtd, &slots, 10.0), Some((0, true)));
        // The server confirmed: the group is on cooldown for 8 s, nothing goes out before.
        b.pocao_grupo(0, 8.0, 5.0, 10.0);
        assert!(b.curando(0, 12.0));
        assert!(b
            .recarga(0, 12.0)
            .is_some_and(|(r, t)| (r - 6.0).abs() < 1e-3 && t == 8.0));
        assert_eq!(b.decide(&e, &qtd, &slots, 12.0), None, "recarga do grupo");
        assert_eq!(b.decide(&e, &qtd, &slots, 17.9), None, "ainda na recarga");
        assert_eq!(
            b.decide(&e, &qtd, &slots, 18.1),
            Some((0, true)),
            "fim da recarga libera"
        );
        // A little is missing (10%) with threshold 95: the smallest that covers, the common one.
        b.ajustar_limiar(0, 35);
        e.hp = 0.9;
        assert_eq!(b.decide(&e, &qtd, &slots, 30.0), Some((0, false)));
        // With no common one in the bag, whichever there is.
        assert_eq!(
            b.decide(&e, &qtd, &[slot(it::GREATER_HEAL, 2)], 40.0),
            Some((0, true))
        );
    }

    #[test]
    fn sem_estoque_desligado_ou_morto_nao_usa_e_buff_so_quando_acaba() {
        let mut b = Barra::default();
        let mut e = cheio();
        e.mp = 0.1;
        assert_eq!(
            b.decide(&e, &[5; ESPACOS], &[], 10.0),
            None,
            "mana comeca manual"
        );
        b.alterna_auto(1);
        assert_eq!(b.decide(&e, &[5, 0, 5, 5], &[], 10.0), None, "sem pocao");
        e.vivo = false;
        assert_eq!(
            b.decide(&e, &[5; ESPACOS], &[], 10.0),
            None,
            "morto nao bebe"
        );
        e.vivo = true;
        assert_eq!(b.decide(&e, &[5; ESPACOS], &[], 10.0), Some((1, false)));
        // Luck in slot 3: only without the buff, and with a long wait.
        b.atribuir(3, it::SORTE_POTION);
        b.alterna_auto(3);
        e.mp = 1.0;
        assert_eq!(b.decide(&e, &[5; ESPACOS], &[], 30.0), None, "buff ativo");
        e.sorte_ativo = false;
        assert_eq!(b.decide(&e, &[5; ESPACOS], &[], 30.0), Some((3, false)));
        assert_eq!(b.decide(&e, &[5; ESPACOS], &[], 35.0), None);
    }

    #[test]
    fn familia_soma_e_escolhe_o_slot_certo() {
        let slots = vec![
            slot(it::GREATER_HEAL, 2),
            slot(it::STEEL, 9),
            slot(it::HEALTH_POTION, 3),
        ];
        assert_eq!(quantidade(&slots, it::HEALTH_POTION), 5);
        assert_eq!(slot_para_usar(&slots, it::HEALTH_POTION, false), Some(2));
        assert_eq!(
            slot_para_usar(&slots, it::HEALTH_POTION, true),
            Some(0),
            "forte usa a Vida+"
        );
        let so_forte = vec![slot(it::GREATER_HEAL, 1)];
        assert_eq!(
            slot_para_usar(&so_forte, it::HEALTH_POTION, false),
            Some(0),
            "sem a comum usa a da familia"
        );
        assert_eq!(slot_para_usar(&slots, it::STEEL, false), None);
    }

    #[test]
    fn servidor_vazio_volta_a_padrao_e_cheio_substitui() {
        let mut b = Barra::default();
        b.limpar(0);
        b.do_servidor(&[]);
        assert_eq!(b.espacos, padrao());
        b.do_servidor(&[espaco(it::FORTUNA_POTION, true)]);
        assert_eq!(b.espacos[0].item_id, it::FORTUNA_POTION);
        assert_eq!(b.espacos[1], EspacoDaBarra::default());
        assert_eq!(b.para_servidor().len(), ESPACOS);
    }
}
