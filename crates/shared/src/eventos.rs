//! BONUS EVENTS: extra XP, drop chance or gold, for everyone or for one map,
//! one mob, one dungeon or one boss, for a set time (owner, 09/11/2026:
//! "1.5x xp for everyone or 2x for this map, drop rate extra for this mob …
//! or even for dungeons and bosses"; "a clear tab in the game to see what is
//! active, and it always goes with a global message every 30 min").
//!
//! The staff creates them in the Panopticon (`eventos_bonus` table). Every
//! game process re-reads the active ones every few seconds, applies them on
//! each kill and tells its players (`ServerMessage::EventosAtivos`), so
//! nothing restarts. Several events that match the same kill MULTIPLY.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tipo {
    Xp,
    Drop,
    Ouro,
}

impl Tipo {
    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "xp" => Some(Tipo::Xp),
            "drop" => Some(Tipo::Drop),
            "ouro" => Some(Tipo::Ouro),
            _ => None,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Xp => "XP",
            Tipo::Drop => "drop rate",
            Tipo::Ouro => "gold & copper",
        }
    }
}

/// Where it applies. The `alvo` of the event says which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Escopo {
    Todos,
    /// `alvo` = zone (prefix: "ilha_magica" covers every Magic Island channel).
    Mapa,
    /// `alvo` = enemy kind id.
    Mob,
    /// `alvo` = dungeon content id; empty = every dungeon.
    Dungeon,
    /// `alvo` = boss kind id; empty = every boss.
    Chefe,
}

impl Escopo {
    pub fn de_texto(s: &str) -> Option<Self> {
        match s {
            "todos" => Some(Escopo::Todos),
            "mapa" => Some(Escopo::Mapa),
            "mob" => Some(Escopo::Mob),
            "dungeon" => Some(Escopo::Dungeon),
            "chefe" => Some(Escopo::Chefe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evento {
    pub id: i64,
    pub tipo: Tipo,
    pub escopo: Escopo,
    /// Raw target (zone, kind id, content id), as stored.
    pub alvo: String,
    /// The target's name for players ("Snow Tiger", "Frozen Hull").
    pub alvo_nome: String,
    pub mult: f32,
    pub inicio_unix: i64,
    pub fim_unix: i64,
}

/// What one kill is: where, what, in which dungeon.
#[derive(Debug, Clone, Copy)]
pub struct Abate<'a> {
    pub zona: &'a str,
    pub kind: u16,
    pub dungeon: Option<u16>,
    pub chefe: bool,
}

impl Evento {
    pub fn vale_para(&self, a: &Abate) -> bool {
        let alvo = self.alvo.trim();
        match self.escopo {
            Escopo::Todos => true,
            Escopo::Mapa => !alvo.is_empty() && a.zona.starts_with(alvo),
            Escopo::Mob => alvo.parse::<u16>().is_ok_and(|k| k == a.kind),
            Escopo::Dungeon => {
                a.dungeon.is_some() && (alvo.is_empty() || alvo.parse::<u16>().ok() == a.dungeon)
            }
            Escopo::Chefe => a.chefe && (alvo.is_empty() || alvo.parse::<u16>().is_ok_and(|k| k == a.kind)),
        }
    }

    pub fn ativo(&self, agora_unix: i64) -> bool {
        self.inicio_unix <= agora_unix && agora_unix < self.fim_unix
    }

    /// "2x XP · Frozen Hull", in English (the client translates).
    pub fn titulo(&self) -> String {
        let onde = match self.escopo {
            Escopo::Todos => "everywhere".to_string(),
            Escopo::Dungeon if self.alvo.trim().is_empty() => "in every dungeon".to_string(),
            Escopo::Chefe if self.alvo.trim().is_empty() => "on every boss".to_string(),
            Escopo::Mapa | Escopo::Dungeon => format!("in {}", self.alvo_nome),
            Escopo::Mob | Escopo::Chefe => format!("on {}", self.alvo_nome),
        };
        format!("{} {} {onde}", mult_texto(self.mult), self.tipo.nome())
    }

    /// The global message that goes with it.
    pub fn anuncio(&self, agora_unix: i64) -> String {
        format!(
            "Bonus event: {}! {} left. Open Menu › Events to see every active bonus.",
            self.titulo(),
            resta_texto(self.fim_unix - agora_unix)
        )
    }
}

pub fn mult_texto(m: f32) -> String {
    if (m - m.round()).abs() < 0.01 {
        format!("{}x", m.round() as i32)
    } else {
        format!("{:.1}x", m)
    }
}

pub fn resta_texto(s: i64) -> String {
    let s = s.max(0);
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{}m", m.max(1))
    }
}

/// The product of every active event of this kind that matches the kill.
pub fn mult(eventos: &[Evento], tipo: Tipo, a: &Abate, agora_unix: i64) -> f32 {
    eventos
        .iter()
        .filter(|e| e.tipo == tipo && e.ativo(agora_unix) && e.vale_para(a))
        .map(|e| e.mult.clamp(0.1, 10.0))
        .product()
}

/// How often an active event is announced again.
pub const LEMBRETE_S: i64 = 30 * 60;

#[cfg(test)]
mod testes {
    use super::*;

    fn ev(tipo: Tipo, escopo: Escopo, alvo: &str, mult: f32) -> Evento {
        Evento {
            id: 1,
            tipo,
            escopo,
            alvo: alvo.into(),
            alvo_nome: "X".into(),
            mult,
            inicio_unix: 0,
            fim_unix: 100,
        }
    }

    #[test]
    fn cada_escopo_pega_so_o_seu_abate_e_eles_multiplicam() {
        let campo = Abate { zona: "ilha_inicial", kind: 3, dungeon: None, chefe: false };
        let dg = Abate { zona: "dungeon", kind: 40, dungeon: Some(5), chefe: true };
        let magica = Abate { zona: "ilha_magica_3", kind: 9, dungeon: None, chefe: false };
        let v = vec![
            ev(Tipo::Xp, Escopo::Todos, "", 1.5),
            ev(Tipo::Xp, Escopo::Mapa, "ilha_magica", 2.0),
            ev(Tipo::Drop, Escopo::Mob, "3", 2.0),
            ev(Tipo::Ouro, Escopo::Dungeon, "5", 3.0),
            ev(Tipo::Drop, Escopo::Chefe, "", 1.5),
        ];
        assert_eq!(mult(&v, Tipo::Xp, &campo, 10), 1.5);
        assert_eq!(mult(&v, Tipo::Xp, &magica, 10), 3.0, "everyone 1.5x times the map 2x");
        assert_eq!(mult(&v, Tipo::Drop, &campo, 10), 2.0);
        assert_eq!(mult(&v, Tipo::Drop, &magica, 10), 1.0);
        assert_eq!(mult(&v, Tipo::Ouro, &dg, 10), 3.0);
        assert_eq!(mult(&v, Tipo::Drop, &dg, 10), 1.5, "any boss");
        assert_eq!(mult(&v, Tipo::Xp, &campo, 100), 1.0, "ended");
    }

    #[test]
    fn titulo_legivel() {
        let mut e = ev(Tipo::Xp, Escopo::Todos, "", 1.5);
        assert_eq!(e.titulo(), "1.5x XP everywhere");
        e.mult = 2.0;
        e.escopo = Escopo::Mapa;
        e.alvo_nome = "Frozen Isle".into();
        assert_eq!(e.titulo(), "2x XP in Frozen Isle");
        assert_eq!(resta_texto(3 * 3600 + 5 * 60), "3h 05m");
    }
}
