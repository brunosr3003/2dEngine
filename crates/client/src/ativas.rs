//! SKILL SLOTS 4–6: the path tree's actives on the HUD (`shared::arvore`,
//! rebirth phase 4).
//!
//! The slots open at rebirths 1, 3 and 5 and hold the actives the player
//! learned and placed (the tree screen's "Equip", `arvore_ui`). Same gesture
//! as the weapon skills (`habilidades`): tap uses, drag up switches AUTO on,
//! drag down switches it off; keys 4, 5 and 6 on desktop. The cooldowns come
//! from the usual `SkillsState` under `arvore::id_de_skill`, and AUTO is kept
//! in the same `Habilidades::automaticas` set (so it persists with the
//! weapon skills' AUTO in the preferences). The server checks everything
//! again (`world/ativas.rs`).
//!
//! The buttons sit in a second arc above the weapon skills, anchored on
//! skill 3's box (so a moved HUD carries them along).

#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::{
    habilidades::{Contexto, Habilidades},
    habilidades_input::{Arrasto, Gesto},
    hud_estilo as estilo,
};
use macroquad::prelude::*;
use shared::arvore::{self as av, Caminho, EstadoArvore, TipoAtiva, SLOTS_EXTRAS};
use std::cell::Cell;

thread_local! {
    /// Open slots with an active, for `botao_em` (the world must not take a
    /// tap on them) — set by `receber`.
    static VISIVEIS: Cell<u8> = const { Cell::new(0) };
    static ARRASTANDO: Cell<bool> = const { Cell::new(false) };
}

/// Where extra slot `slot` (0 = skill 4) is drawn: over skill 3, then up and
/// to the left, clear of the weapon arc and the jump.
pub fn retangulo(slot: usize) -> Rect {
    let z = crate::hud_layout::atual();
    let s = z.s;
    let c3 = z.skills[2].center();
    let d =
        [vec2(0.0, -100.0), vec2(-100.0, -100.0), vec2(-200.0, -80.0)][slot.min(SLOTS_EXTRAS - 1)];
    let c = c3 + d * s;
    let rs = 36.0 * s;
    Rect::new(c.x - rs, c.y - rs, rs * 2.0, rs * 2.0)
}

/// Does `p` land on a visible slot button?
pub fn botao_em(p: Vec2) -> bool {
    let n = VISIVEIS.with(Cell::get) as usize;
    (0..n.min(SLOTS_EXTRAS)).any(|i| retangulo(i).toca(p))
}

/// A slot gesture in progress, or the mouse over a slot.
pub fn pega_mouse() -> bool {
    ARRASTANDO.with(Cell::get) || botao_em(Vec2::from(mouse_position()))
}

/// The weapon skill whose existing effect stands in for each active's
/// (`habilidades_vfx`), or `None` (Shadowstep and Invisibility show on the
/// body itself).
pub fn fx_da_ativa(ativa: u8) -> Option<u32> {
    match av::def(ativa)?.tipo {
        TipoAtiva::Passo | TipoAtiva::Invisibilidade => None,
        TipoAtiva::Furia => Some(11),
        TipoAtiva::Nova => Some(9),
        TipoAtiva::Meteoro => Some(12),
        TipoAtiva::Tempo | TipoAtiva::Saque => Some(11),
        TipoAtiva::Provocar | TipoAtiva::Escudo | TipoAtiva::Inquebravel => Some(3),
    }
}

/// The weapon skill whose icon the slot borrows (no art of their own yet).
pub(crate) fn icone_da_ativa(ativa: u8) -> u32 {
    match ativa {
        0 => 1,
        1 => 5,
        2 => 7,
        3 => 9,
        4 => 12,
        5 => 11,
        6 => 2,
        7 | 8 => 3,
        _ => 8,
    }
}

pub fn cor_do_caminho(c: Caminho) -> Color {
    match c {
        Caminho::Assassino => Color::new(0.95, 0.42, 0.40, 1.0),
        Caminho::Mago => Color::new(0.52, 0.66, 1.0, 1.0),
        Caminho::Guardiao => Color::new(0.93, 0.78, 0.42, 1.0),
        Caminho::Corsario => Color::new(0.36, 0.86, 0.78, 1.0),
    }
}

#[derive(Default)]
pub struct Ativas {
    /// The active in each extra slot (from `EstadoArvore::slots`).
    pub slots: [Option<u8>; SLOTS_EXTRAS],
    pub abertos: u8,
    pub renascimentos: u32,
    arrasto: Arrasto,
    /// A request went out: wait for the answer (or this long).
    pendente_ate: f64,
    tentar_apos: [f64; SLOTS_EXTRAS],
    /// My buff running: (active, start, end) — the ring on the button.
    efeito: Option<(u8, f64, f64)>,
}

impl Ativas {
    pub fn receber(&mut self, e: &EstadoArvore) {
        self.slots = e.slots;
        self.abertos = e.slots_abertos.min(SLOTS_EXTRAS as u8);
        self.renascimentos = e.renascimentos;
        VISIVEIS.with(|v| v.set(self.abertos));
    }

    /// My own `AtivaFx`: the answer arrived; a buff starts its ring.
    pub fn confirmada(&mut self, ativa: u8, duracao_s: f32) {
        self.pendente_ate = 0.0;
        if duracao_s > 0.0 {
            let agora = get_time();
            self.efeito = Some((ativa, agora, agora + duracao_s as f64));
        }
    }

    /// The server refused (`SkillRejected` with an active's id).
    pub fn rejeitada(&mut self, id: u32) {
        self.pendente_ate = 0.0;
        if let Some(i) =
            av::ativa_do_id(id).and_then(|a| self.slots.iter().position(|s| *s == Some(a)))
        {
            self.tentar_apos[i] = get_time() + 2.5;
        }
    }

    fn ativa(&self, slot: usize) -> Option<u8> {
        (slot < self.abertos as usize)
            .then(|| self.slots[slot])
            .flatten()
    }

    /// Why active `a` cannot go now (English, the HUD translates), or `None`.
    fn impedimento(
        &self,
        a: u8,
        hab: &Habilidades,
        c: Contexto,
        agora: f64,
    ) -> Option<&'static str> {
        let d = av::def(a)?;
        if !c.vivo {
            Some("Character incapacitated")
        } else if hab.recarga_ate(av::id_de_skill(a)) > agora {
            Some("Skill on cooldown")
        } else if c.mp < d.custo_mp {
            Some("Not enough mana")
        } else if matches!(d.tipo, TipoAtiva::Meteoro) && c.distancia_alvo.is_none() {
            Some("Select an enemy to use this skill")
        } else {
            None
        }
    }

    /// The slot to use this frame (`ClientMessage::UsarAtiva`), from a tap,
    /// a key or AUTO. With the input blocked (power saving, a panel), AUTO
    /// only — the same rule as the weapon skills.
    pub fn pedido(
        &mut self,
        hab: &mut Habilidades,
        c: Contexto,
        entrada_bloqueada: bool,
    ) -> Option<u8> {
        let agora = get_time();
        let n = self.abertos as usize;
        if n == 0 {
            self.arrasto.cancela();
            ARRASTANDO.with(|a| a.set(false));
            return None;
        }
        let mut manual: Option<usize> = None;
        if entrada_bloqueada {
            self.arrasto.cancela();
        } else {
            let mouse = Vec2::from(mouse_position());
            if crate::foco::clique() {
                if let Some(i) = (0..n).find(|&i| retangulo(i).toca(mouse)) {
                    // The id carries the SLOT (an empty slot still answers).
                    self.arrasto.pressiona(i as u32, mouse);
                }
            }
            self.arrasto.move_para(mouse);
            let mut gesto = None;
            if let Some((i, _)) = self.arrasto.inicio {
                if is_mouse_button_released(MouseButton::Left) {
                    gesto = self.arrasto.solta(mouse, retangulo(i as usize));
                } else if !is_mouse_button_down(MouseButton::Left) {
                    self.arrasto.cancela();
                }
            }
            match gesto {
                Some(Gesto::Auto(i, ligar)) => {
                    match self.ativa(i as usize) {
                        None => hab.aviso("Equip an active here from the path tree".into()),
                        Some(a) if ligar && !auto_faz_sentido(a) => {
                            hab.aviso("This active is manual only".into())
                        }
                        Some(a) => {
                            let id = av::id_de_skill(a);
                            if ligar {
                                hab.automaticas.insert(id);
                                hab.ligou_auto = true;
                            } else {
                                hab.automaticas.remove(&id);
                            }
                            hab.aviso(
                                if ligar {
                                    "Automatic use switched on"
                                } else {
                                    "Automatic use switched off"
                                }
                                .into(),
                            );
                        }
                    }
                    ARRASTANDO.with(|a| a.set(false));
                    return None;
                }
                Some(Gesto::Usar(i)) => manual = Some(i as usize),
                None => {}
            }
            if manual.is_none() {
                manual = (0..n).find(|&i| crate::desktop::skill_pressionada(3 + i));
            }
        }
        ARRASTANDO.with(|a| a.set(self.arrasto.inicio.is_some()));
        if let Some(i) = manual {
            let Some(a) = self.ativa(i) else {
                hab.aviso("Equip an active here from the path tree".into());
                return None;
            };
            if let Some(m) = self.impedimento(a, hab, c, agora) {
                hab.aviso(m.into());
                return None;
            }
            if agora < self.pendente_ate {
                return None;
            }
            self.pendente_ate = agora + 0.8;
            return Some(i as u8);
        }
        if self.arrasto.inicio.is_some() || agora < self.pendente_ate {
            return None;
        }
        // AUTO: the first slot whose rule holds.
        for i in 0..n {
            let Some(a) = self.ativa(i) else { continue };
            if !hab.automaticas.contains(&av::id_de_skill(a))
                || agora < self.tentar_apos[i]
                || self.impedimento(a, hab, c, agora).is_some()
                || !auto_agora(a, c)
            {
                continue;
            }
            self.pendente_ate = agora + 0.8;
            self.tentar_apos[i] = agora + 1.5;
            return Some(i as u8);
        }
        None
    }

    pub fn desenha(&self, hab: &Habilidades, mp: i32) {
        let agora = get_time();
        for i in 0..self.abertos as usize {
            let r = retangulo(i);
            let c = r.center();
            let raio = r.w * 0.5;
            let sobre = r.toca(Vec2::from(mouse_position()));
            let Some(a) = self.ativa(i) else {
                // An empty open slot: a faint ring and its number.
                estilo::botao_redondo(
                    c,
                    raio,
                    estilo::SUAVE,
                    estilo::estado(sobre, false, true, false),
                    false,
                );
                estilo::texto_centro(c.x, c.y + 6.0, "+", 20, estilo::SUAVE);
                crate::hud_layout::chip(r, &(i + 4).to_string());
                continue;
            };
            let d = av::def(a).expect("a known active");
            let cor = av::no_da_ativa(a).map_or(estilo::OURO, |(cam, _)| cor_do_caminho(cam));
            let id = av::id_de_skill(a);
            let cd = (hab.recarga_ate(id) - agora).max(0.0);
            let pronto = cd == 0.0 && mp >= d.custo_mp && hab.recarga_ate(id) <= agora;
            let auto = hab.automaticas.contains(&id);
            let e = estilo::estado(sobre, is_mouse_button_down(MouseButton::Left), false, false);
            estilo::botao_redondo(
                c,
                raio,
                if auto { estilo::AUTO } else { cor },
                e,
                pronto && auto,
            );
            let alfa = if pronto { 1.0 } else { 0.40 };
            let icone = icone_da_ativa(a);
            if !crate::icones_ui::skill(icone, c, raio * 1.4, alfa) {
                estilo::icone(icone, c, raio * 0.55, Color::new(cor.r, cor.g, cor.b, alfa));
            }
            // The path's colour ring, so it never reads as a weapon skill.
            estilo::arco(c, raio + 2.0, 0.0, 1.0, 2.0, estilo::alfa(cor, 0.9));
            if av::e_suprema(a, self.renascimentos) {
                estilo::arco(c, raio + 5.0, 0.0, 1.0, 1.5, estilo::OURO);
            }
            // A buff of mine running: the ring empties with what is left.
            if let Some((ef, ini, fim)) = self.efeito.filter(|e| e.0 == a && agora < e.2) {
                let _ = ef;
                let f = ((fim - agora) / (fim - ini).max(0.01)) as f32;
                estilo::arco(
                    c,
                    raio + 8.0,
                    -std::f32::consts::FRAC_PI_2,
                    f,
                    3.0,
                    estilo::AUTO,
                );
            }
            if cd > 0.0 {
                let espera = shared::skills::espera_efetiva(d.espera_s, hab.wis);
                let f = (cd as f32 / espera.max(0.01)).min(1.0);
                estilo::setor(c, raio - 3.0, f, Color::new(0.0, 0.0, 0.0, 0.62));
                let t = format!("{cd:.0}");
                estilo::texto_sombra(
                    c.x - estilo::medir_forte(&t, 20) * 0.5,
                    c.y + 7.0,
                    &t,
                    20,
                    WHITE,
                    true,
                );
            }
            crate::hud_layout::chip(r, &(i + 4).to_string());
            if auto {
                estilo::texto_centro_forte(c.x, r.y + r.h + 3.0, "AUTO", 11, estilo::AUTO);
            }
            if sobre && self.arrasto.inicio.is_none() {
                let w = 380.0_f32.min(screen_width() - 24.0);
                let x = (r.x + r.w - w).clamp(12.0, screen_width() - w - 12.0);
                let y = (r.y - 104.0).max(8.0);
                estilo::painel(Rect::new(x, y, w, 92.0));
                estilo::texto(x + 12.0, y + 24.0, av::nome_da_ativa(a), 19, cor);
                estilo::texto_ajustado(
                    d.descricao,
                    x + 12.0,
                    y + 45.0,
                    w - 24.0,
                    13,
                    estilo::TEXTO,
                );
                estilo::texto(
                    x + 12.0,
                    y + 82.0,
                    &format!(
                        "{} MP  ·  {:.0}s",
                        d.custo_mp,
                        shared::skills::espera_efetiva(d.espera_s, hab.wis)
                    ),
                    13,
                    estilo::SUAVE,
                );
            }
        }
    }
}

/// Can AUTO run this active at all? The movement and the vanish are the
/// player's call.
fn auto_faz_sentido(a: u8) -> bool {
    !matches!(
        av::def(a).map(|d| d.tipo),
        Some(TipoAtiva::Passo | TipoAtiva::Invisibilidade) | None
    )
}

/// AUTO's rule for each active (the cooldown and mana are checked apart).
fn auto_agora(a: u8, c: Contexto) -> bool {
    let Some(d) = av::def(a) else { return false };
    match d.tipo {
        TipoAtiva::Passo | TipoAtiva::Invisibilidade => false,
        TipoAtiva::Meteoro => c.distancia_alvo.is_some_and(|x| x <= d.alcance),
        // Lethal Fury: in a fight (a target in reach of the basic attack).
        TipoAtiva::Furia => c.distancia_alvo.is_some_and(|x| x <= 8.0),
        TipoAtiva::Nova | TipoAtiva::Provocar => c.distancia_alvo.is_some_and(|x| x <= d.raio),
        TipoAtiva::Escudo | TipoAtiva::Inquebravel => c.vida_baixa,
        TipoAtiva::Tempo | TipoAtiva::Saque => c.distancia_alvo.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::skills::Conjunto;

    fn ctx() -> Contexto {
        Contexto {
            conjunto: Conjunto::Katana,
            nivel: 20,
            mp: 100,
            vivo: true,
            vida_baixa: false,
            distancia_alvo: Some(3.0),
        }
    }

    #[test]
    fn so_os_slots_abertos_valem() {
        let mut a = Ativas::default();
        let mut e = EstadoArvore::default().com_renascimentos(3);
        e.slots = [Some(3), Some(4), Some(5)];
        a.receber(&e);
        assert_eq!(a.abertos, 2);
        assert_eq!(
            (a.ativa(0), a.ativa(1), a.ativa(2)),
            (Some(3), Some(4), None),
            "slot 6 opens at rebirth 5"
        );
    }

    #[test]
    fn regras_do_auto() {
        let mut c = ctx();
        assert!(auto_agora(3, c), "Arcane Nova: target within 4");
        c.distancia_alvo = Some(5.0);
        assert!(!auto_agora(3, c));
        assert!(auto_agora(4, c), "Meteor reaches 10");
        assert!(!auto_agora(7, c), "Bulwark Shield waits for low health");
        c.vida_baixa = true;
        assert!(auto_agora(7, c));
        assert!(
            !auto_faz_sentido(1) && !auto_faz_sentido(0),
            "Invisibility and Shadowstep are manual"
        );
        assert!(auto_faz_sentido(9));
        c.distancia_alvo = None;
        assert!(!auto_agora(9, c), "the Corsair buff waits for a fight");
    }

    #[test]
    fn impedimentos() {
        let a = Ativas::default();
        let h = Habilidades::default();
        let mut c = ctx();
        assert_eq!(a.impedimento(3, &h, c, 10.0), None);
        c.mp = 5;
        assert_eq!(a.impedimento(3, &h, c, 10.0), Some("Not enough mana"));
        c.mp = 100;
        c.distancia_alvo = None;
        // Meteor (4) needs a target; Lethal Fury (2) is a buff and does not.
        assert_eq!(
            a.impedimento(4, &h, c, 10.0),
            Some("Select an enemy to use this skill")
        );
        assert_eq!(a.impedimento(2, &h, c, 10.0), None);
        assert_eq!(
            a.impedimento(7, &h, c, 10.0),
            None,
            "a buff needs no target"
        );
        c.vivo = false;
        assert!(a.impedimento(7, &h, c, 10.0).is_some());
        assert!(fx_da_ativa(1).is_none() && fx_da_ativa(4) == Some(12));
    }
}
