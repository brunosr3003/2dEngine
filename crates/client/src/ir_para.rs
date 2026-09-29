//! "Go to": from the map menu or the quest menu, the character walks there
//! alone and, on arriving, does what the place asks — switches on auto combat
//! in the creature's zone, auto gathering in the resource's region, or talks
//! to the NPC.
//!
//! The travel is the map's (legs, a dashed line on the ground, automatic
//! running): here we only decide the next step, with no macroquad in the core.
use macroquad::prelude::*;

/// The travel ended far from the target: ask again after this.
const RELIGA_S: f64 = 1.0;
/// Within this of the NPC: arrived (the shop's "go to the NPC" closes the path).
const PERTO_DO_NPC: f32 = 4.5;
/// Zones and regions are large; arriving means entering them, not stepping on the center.
const CHEGOU_MAX: f32 = 12.0;
/// Repeated requests without getting closer than this: give up.
const DESISTE_APOS: u32 = 8;
const PROGRESSO_MINIMO: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Objetivo {
    /// Zona de bicho: auto combate centrado nela.
    Combate,
    /// Resource region of the type (0 wood, 1..4 stone by color).
    Coleta(u8),
    /// An NPC: talk to them on arrival.
    Npc,
    /// Just get there (a boss far above the level: nothing switches on by itself).
    Lugar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alvo {
    pub objetivo: Objetivo,
    pub pos: Vec2,
    pub raio: f32,
    /// "Wolf", "Blue stone", "Quest Master": what the HUD shows.
    pub rotulo: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Acao {
    Viajar(Vec2),
    LigarCombate(Vec2),
    LigarColeta(u8, Vec2),
    /// Talk to whichever NPC is near this point.
    FalarPerto(Vec2),
    Aviso(String),
}

#[derive(Debug, Default)]
pub struct IrPara {
    alvo: Option<Alvo>,
    desde: f64,
    pedidos_sem_progresso: u32,
    melhor: f32,
    /// "Nothing is changing" — see `parado.rs`.
    parado: crate::parado::Parado,
}

impl IrPara {
    pub fn ativo(&self) -> bool {
        self.alvo.is_some()
    }

    pub fn alvo(&self) -> Option<&Alvo> {
        self.alvo.as_ref()
    }

    pub fn iniciar(&mut self, alvo: Alvo, agora: f64) {
        *self = Self {
            alvo: Some(alvo),
            desde: agora - RELIGA_S,
            pedidos_sem_progresso: 0,
            melhor: f32::MAX,
            parado: crate::parado::Parado::default(),
        };
    }

    pub fn parar(&mut self) {
        *self = Self::default();
    }

    fn alcance(a: &Alvo) -> f32 {
        match a.objetivo {
            Objetivo::Npc => PERTO_DO_NPC,
            _ => a.raio.clamp(3.0, CHEGOU_MAX),
        }
    }

    /// One frame. `viajando` = the map's travel is still running.
    pub fn passo(&mut self, eu: Vec2, agora: f64, viajando: bool) -> Option<Acao> {
        let a = self.alvo.as_ref()?;
        let d = eu.distance(a.pos);
        if d <= Self::alcance(a) {
            let acao = match a.objetivo {
                Objetivo::Combate => Acao::LigarCombate(a.pos),
                Objetivo::Coleta(t) => Acao::LigarColeta(t, a.pos),
                Objetivo::Npc => Acao::FalarPerto(a.pos),
                Objetivo::Lugar => Acao::Aviso(format!("Arrived: {}.", a.rotulo)),
            };
            self.parar();
            return Some(acao);
        }
        // TRAVADO CONTA MESMO "VIAJANDO".
        //
        // `viajando` on its own made this step never run while the body pushed
        // against a wall: the travel stays "active" and the `pedidos_sem_progresso`
        // count — which is what gives up — never advanced.
        //
        // A third file with the same assumption (see `parado.rs`): `auto_missao.rs`
        // and `auto_coleta.rs` each had a copy of it.
        self.parado.acompanha(eu, agora);
        let travado = self.parado.travado(agora);
        if (viajando && !travado) || agora - self.desde < RELIGA_S {
            return None;
        }
        if travado {
            self.parado.zera(agora);
        }
        self.desde = agora;
        if d < self.melhor - PROGRESSO_MINIMO {
            self.melhor = d;
            self.pedidos_sem_progresso = 0;
        } else {
            self.pedidos_sem_progresso += 1;
            if self.pedidos_sem_progresso >= DESISTE_APOS {
                let rotulo = a.rotulo.clone();
                self.parar();
                return Some(Acao::Aviso(format!("I found no path to {rotulo}.")));
            }
        }
        // NPC: stop beside them, not on top.
        let destino = match a.objetivo {
            Objetivo::Npc => a.pos + (eu - a.pos).normalize_or_zero() * 2.0,
            _ => a.pos,
        };
        Some(Acao::Viajar(destino))
    }

    /// O texto da faixa de estado unica do HUD.
    pub fn faixa(&self, eu: Option<Vec2>) -> Option<String> {
        let (Some(a), Some(eu)) = (&self.alvo, eu) else {
            return None;
        };
        Some(format!("HEADING · {} · {:.0} m", a.rotulo, eu.distance(a.pos)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alvo(objetivo: Objetivo, pos: Vec2, raio: f32) -> Alvo {
        Alvo {
            objetivo,
            pos,
            raio,
            rotulo: "x".into(),
        }
    }

    #[test]
    fn longe_viaja_e_perto_liga_o_que_o_lugar_pede() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(300.0, 0.0), 45.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(300.0, 0.0)))
        );
        // Travelling: do not ask again.
        assert_eq!(ir.passo(vec2(100.0, 0.0), 0.5, true), None);
        // Inside the zone (within 12 of the center): switch combat on and finish.
        assert_eq!(
            ir.passo(vec2(290.0, 0.0), 3.0, true),
            Some(Acao::LigarCombate(vec2(300.0, 0.0)))
        );
        assert!(!ir.ativo());

        ir.iniciar(alvo(Objetivo::Coleta(3), vec2(0.0, 50.0), 8.0), 0.0);
        assert_eq!(
            ir.passo(vec2(0.0, 44.0), 0.0, false),
            Some(Acao::LigarColeta(3, vec2(0.0, 50.0)))
        );
    }

    #[test]
    fn npc_para_do_lado_e_fala_ao_chegar() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Npc, vec2(20.0, 0.0), 0.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(18.0, 0.0)))
        );
        assert_eq!(
            ir.passo(vec2(17.0, 0.0), 2.0, false),
            Some(Acao::FalarPerto(vec2(20.0, 0.0)))
        );
    }

    #[test]
    fn sem_chegar_mais_perto_desiste() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(500.0, 0.0), 45.0), 0.0);
        let mut aviso = None;
        for k in 0..20 {
            match ir.passo(vec2(100.0, 0.0), k as f64 * 1.1, false) {
                Some(Acao::Aviso(s)) => {
                    aviso = Some(s);
                    break;
                }
                Some(Acao::Viajar(_)) => {}
                outra => panic!("inesperado: {outra:?}"),
            }
        }
        assert!(aviso.is_some(), "nao desistiu");
        assert!(!ir.ativo());
    }

    #[test]
    fn lugar_so_chega_e_avisa() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Lugar, vec2(40.0, 0.0), 6.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(40.0, 0.0)))
        );
        assert_eq!(
            ir.passo(vec2(36.0, 0.0), 2.0, false),
            Some(Acao::Aviso("Arrived: x.".into()))
        );
        assert!(!ir.ativo());
    }

    #[test]
    fn parar_cancela() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(500.0, 0.0), 45.0), 0.0);
        ir.parar();
        assert_eq!(ir.passo(vec2(499.0, 0.0), 5.0, false), None);
    }
}
