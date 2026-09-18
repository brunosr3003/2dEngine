//! Pra onde o corpo OLHA, no fio (`EntityState::rumo`).
//!
//! O cliente tirava o rumo so' da velocidade: quem estava de fora via o outro
//! jogador parado coletando, atacando ou mirando olhando pro ultimo passo. O
//! servidor e' quem sabe pra onde o corpo esta' virado, entao ele escreve.
use glam::Vec2;

/// Abaixo disto (u/s) o corpo esta' parado: a velocidade nao diz rumo.
pub const VEL_MINIMA: f32 = 0.2;

/// O rumo de um corpo neste tick, na ordem do que manda mais:
///
///  1. `mira` — um PONTO pra onde ele olha (o no' que coleta, o alvo que ataca);
///  2. `golpe` — a DIRECAO de um golpe em curso (o mob que morde);
///  3. a velocidade, andando;
///  4. `fixo` — o rumo parado de quem nao se move (NPC da vila);
///
/// e 0 (sem rumo: o cliente mantem o que tinha) se nada disso vale.
pub fn escolhe(
    pos: Vec2,
    mira: Option<Vec2>,
    golpe: Option<Vec2>,
    vel: Vec2,
    fixo: Option<f32>,
) -> u8 {
    if let Some(p) = mira {
        let r = shared::rumo_de_dir(p - pos);
        if r != 0 {
            return r;
        }
    }
    if let Some(d) = golpe {
        let r = shared::rumo_de_dir(d);
        if r != 0 {
            return r;
        }
    }
    if vel.length() > VEL_MINIMA {
        return shared::rumo_de_dir(vel);
    }
    fixo.map_or(0, shared::rumo_de_yaw)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn yaw(r: u8) -> f32 {
        shared::yaw_de_rumo(r).expect("sem rumo")
    }

    #[test]
    fn cada_estado_escolhe_o_rumo_certo() {
        let pos = Vec2::new(10.0, 10.0);
        let q = std::f32::consts::FRAC_PI_2;
        // Coletando/atacando: olha pro PONTO, mesmo andando pro outro lado.
        let r = escolhe(
            pos,
            Some(Vec2::new(20.0, 10.0)),
            None,
            Vec2::new(0.0, -3.0),
            None,
        );
        assert!((yaw(r) - q).abs() < 0.03, "olha pro no' (+X)");
        // Mob mordendo: a direcao do golpe vence a velocidade.
        let r = escolhe(
            pos,
            None,
            Some(Vec2::new(0.0, 1.0)),
            Vec2::new(-3.0, 0.0),
            None,
        );
        assert!(yaw(r).abs() < 0.03, "olha pro golpe (+Z)");
        // Andando: pra onde anda.
        let r = escolhe(pos, None, None, Vec2::new(-2.0, 0.0), None);
        assert!((yaw(r) - 3.0 * q).abs() < 0.03, "olha pra onde anda (-X)");
        // Parado sem nada: NPC mantem o fixo; o resto fica sem rumo.
        assert!((yaw(escolhe(pos, None, None, Vec2::ZERO, Some(q))) - q).abs() < 0.03);
        assert_eq!(escolhe(pos, None, None, Vec2::new(0.05, 0.0), None), 0);
        // Mira em cima do proprio corpo nao diz nada: cai pro resto.
        assert_eq!(escolhe(pos, Some(pos), None, Vec2::ZERO, None), 0);
    }
}
