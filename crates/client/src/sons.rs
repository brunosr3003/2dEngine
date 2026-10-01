//! Embedded CC0 effects: the same pack on desktop and on iPhone.
use macroquad::{
    audio::{load_sound_from_bytes, play_sound, stop_sound, PlaySoundParams, Sound},
    prelude::*,
};
use std::cell::RefCell;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Som {
    Golpe,
    Golpe2,
    Machado,
    Item,
    Acerto,
    Acerto2,
    Critico,
    Dor,
    Picareta,
    Morte,
    Clique,
    Recusa,
    Progresso,
    Pronta,
    Recompensa,
    Magia,
    Impacto,
    Disparo,
    MobAtaque,
    MobDor,
    MobMorte,
    MobDisparo,
    MobMagia,
    MobFlecha,
    Skill1Cast,
    Skill1Impact,
    Skill2Cast,
    Skill2Impact,
    Skill3Cast,
    Skill3Impact,
    Skill4Cast,
    Skill4Impact,
    Skill5Cast,
    Skill5Impact,
    Skill6Cast,
    Skill6Impact,
    Skill7Cast,
    Skill7Impact,
    Skill8Cast,
    Skill8Impact,
    Skill9Cast,
    Skill9Impact,
    Skill10Cast,
    Skill10Impact,
    Skill11Cast,
    Skill11Impact,
    Skill12Cast,
    Skill12Impact,
}
const SOM_COUNT: usize = Som::Skill12Impact as usize + 1;
struct Audio {
    sons: Vec<Option<Sound>>,
    ultimo: [f64; SOM_COUNT],
    vozes: Vec<(usize, f64, bool)>,
    volume: f32,
}
thread_local! { static AUDIO: RefCell<Option<Audio>> = const { RefCell::new(None) }; }
macro_rules! som {
    ($n:literal) => {
        include_bytes!(concat!("../../../assets/audio/", $n, ".ogg")).as_slice()
    };
}
pub async fn carregar() {
    let arquivos = [
        som!("combat/corte_1"),
        som!("combat/corte_2"),
        som!("chop"),
        som!("handleCoins2"),
        som!("combat/acerto_1"),
        som!("combat/acerto_2"),
        som!("combat/critico"),
        som!("impactSoft_heavy_000"),
        som!("impactPlate_light_000"),
        som!("impactSoft_medium_000"),
        som!("click_001"),
        som!("error_001"),
        som!("pluck_001"),
        som!("confirmation_002"),
        som!("confirmation_004"),
        som!("combat/magia"),
        som!("combat/magia_impacto"),
        som!("combat/pistola"),
        som!("combat/mob_ataque"),
        som!("combat/mob_dor"),
        som!("combat/mob_morte"),
        som!("combat/pistola"),
        som!("combat/magia"),
        som!("combat/corte_2"),
        som!("skills/01_cast"),
        som!("skills/01_impact"),
        som!("skills/02_cast"),
        som!("skills/02_impact"),
        som!("skills/03_cast"),
        som!("skills/03_impact"),
        som!("skills/04_cast"),
        som!("skills/04_impact"),
        som!("skills/05_cast"),
        som!("skills/05_impact"),
        som!("skills/06_cast"),
        som!("skills/06_impact"),
        som!("skills/07_cast"),
        som!("skills/07_impact"),
        som!("skills/08_cast"),
        som!("skills/08_impact"),
        som!("skills/09_cast"),
        som!("skills/09_impact"),
        som!("skills/10_cast"),
        som!("skills/10_impact"),
        som!("skills/11_cast"),
        som!("skills/11_impact"),
        som!("skills/12_cast"),
        som!("skills/12_impact"),
    ];
    debug_assert_eq!(arquivos.len(), SOM_COUNT);
    let mut sons = Vec::new();
    for bytes in arquivos {
        sons.push(load_sound_from_bytes(bytes).await.ok());
    }
    let volume = crate::lembranca::caminho()
        .and_then(|p| std::fs::read_to_string(p.with_file_name("audio.prefs")).ok())
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.6)
        .clamp(0., 1.);
    AUDIO.with(|a| {
        *a.borrow_mut() = Some(Audio {
            sons,
            ultimo: [-100.; SOM_COUNT],
            vozes: Vec::new(),
            volume,
        })
    });
}
fn ganho(distancia: f32, proprio: bool) -> f32 {
    if proprio {
        1.0
    } else {
        (1. - distancia / 32.).clamp(0., 1.) * 0.3
    }
}
pub fn mundo(s: Som, pos: Vec2, ouvinte: Option<Vec2>, proprio: bool) {
    if let Some(p) = ouvinte {
        reproduzir(s, ganho(p.distance(pos), proprio), proprio);
    }
}
/// Mobs involved in local combat get their own reserve per effect and an
/// audible volume; they are still attenuated by distance, including on death.
pub fn mob(s: Som, pos: Vec2, ouvinte: Option<Vec2>, envolvido: bool) {
    if let Some(p) = ouvinte {
        let distancia = p.distance(pos);
        let volume = (1.0 - distancia / 32.0).clamp(0.0, 1.0) * if envolvido { 0.85 } else { 0.45 };
        reproduzir(s, volume, envolvido);
    }
}
pub fn ataque_mob(kind: u16) -> Som {
    match shared::bestiary::species_of(shared::bosses::kind_do_corpo(kind)) {
        2 => Som::MobDisparo,
        6 => Som::MobFlecha,
        4 => Som::MobMagia,
        _ => Som::MobAtaque,
    }
}
/// Stable id from the server's catalogue; each phase uses its own file.
fn par_skill(id: u32) -> Option<(Som, Som)> {
    Some(match id {
        1 => (Som::Skill1Cast, Som::Skill1Impact),
        2 => (Som::Skill2Cast, Som::Skill2Impact),
        3 => (Som::Skill3Cast, Som::Skill3Impact),
        4 => (Som::Skill4Cast, Som::Skill4Impact),
        5 => (Som::Skill5Cast, Som::Skill5Impact),
        6 => (Som::Skill6Cast, Som::Skill6Impact),
        7 => (Som::Skill7Cast, Som::Skill7Impact),
        8 => (Som::Skill8Cast, Som::Skill8Impact),
        9 => (Som::Skill9Cast, Som::Skill9Impact),
        10 => (Som::Skill10Cast, Som::Skill10Impact),
        11 => (Som::Skill11Cast, Som::Skill11Impact),
        12 => (Som::Skill12Cast, Som::Skill12Impact),
        _ => return None,
    })
}
pub fn skill(id: u32, impacto: bool, pos: Vec2, ouvinte: Option<Vec2>, proprio: bool) {
    if let Some((cast, hit)) = par_skill(id) {
        mundo(if impacto { hit } else { cast }, pos, ouvinte, proprio);
    }
}
pub fn tocar(s: Som) {
    reproduzir(s, 1., true);
}
fn reproduzir(s: Som, ganho: f32, prioridade: bool) {
    if ganho <= 0. {
        return;
    }
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        let Some(a) = a.as_mut() else { return };
        let i = s as usize;
        let agora = get_time();
        if !prioridade && a.vozes.iter().any(|v| v.0 == i && v.2 && agora - v.1 < 1.5) {
            return;
        }
        let intervalo = match s {
            Som::MobDor => 0.65,
            Som::MobAtaque => 0.55,
            Som::MobMorte => 0.35,
            Som::Progresso => 1.2,
            Som::Recusa => 1.5,
            Som::Pronta | Som::Recompensa => 0.6,
            Som::Clique => 0.08,
            _ => 0.16,
        };
        if a.volume <= 0. || agora - a.ultimo[i] < intervalo || a.sons[i].is_none() {
            return;
        }
        // Reserves four voices for the player themselves. Every voice is stopped
        // before being removed: the limit does not depend on the file's duration.
        let mut n = 0;
        while n < a.vozes.len() {
            if agora - a.vozes[n].1 >= 1.5 || a.vozes[n].0 == i {
                if let Some(s) = &a.sons[a.vozes[n].0] {
                    stop_sound(s)
                }
                a.vozes.remove(n);
            } else {
                n += 1;
            }
        }
        let Ok(remover) = vaga(&a.vozes, prioridade) else {
            return;
        };
        if let Some(n) = remover {
            if let Some(s) = &a.sons[a.vozes[n].0] {
                stop_sound(s)
            }
            a.vozes.remove(n);
        }
        play_sound(
            a.sons[i].as_ref().unwrap(),
            PlaySoundParams {
                looped: false,
                volume: a.volume
                    * ganho
                    * 0.65
                    * match s {
                        Som::Golpe | Som::Golpe2 => 0.75,
                        Som::Acerto | Som::Acerto2 | Som::Dor => 0.6,
                        Som::Magia | Som::Impacto | Som::Disparo => 0.8,
                        _ => 1.0,
                    },
            },
        );
        a.ultimo[i] = agora;
        a.vozes.push((i, agora, prioridade));
    });
}
// Returns the voice to replace, or refuses external sounds when the reserve fills.
fn vaga(vozes: &[(usize, f64, bool)], prioridade: bool) -> Result<Option<usize>, ()> {
    if !prioridade && vozes.iter().filter(|v| !v.2).count() >= 2 {
        return Err(());
    }
    if vozes.len() < 6 {
        return Ok(None);
    }
    vozes
        .iter()
        .position(|v| !v.2)
        .or_else(|| prioridade.then_some(0))
        .map(Some)
        .ok_or(())
}
/// Control available in both versions of the Interface panel.
pub fn controle(r: Rect) {
    let v = AUDIO.with(|a| a.borrow().as_ref().map_or(0., |a| a.volume));
    let texto = if v == 0. {
        "Sound: off".to_string()
    } else {
        format!("Som: {:.0}%", v * 100.)
    };
    if crate::ui::botao(r, &texto, true) {
        let novo = if v >= 0.99 { 0. } else { (v + 0.2).min(1.) };
        AUDIO.with(|a| {
            if let Some(a) = a.borrow_mut().as_mut() {
                a.volume = novo;
                for s in a.sons.iter().flatten() {
                    stop_sound(s)
                }
                a.vozes.clear();
            }
        });
        if let Some(p) = crate::lembranca::caminho() {
            let _ = std::fs::write(p.with_file_name("audio.prefs"), novo.to_string());
        }
        tocar(Som::Pronta);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn todas_as_skills_tem_arquivos_e_canais_exclusivos() {
        let mut usados = std::collections::HashSet::new();
        for skill in shared::skills::playtest() {
            let (cast, hit) = par_skill(skill.id).expect("skill sem audio");
            assert!(usados.insert(cast as usize));
            assert!(usados.insert(hit as usize));
        }
        assert_eq!(usados.len(), shared::skills::SKILL_COUNT * 2);
        assert!(par_skill(0).is_none());
        assert!(par_skill(999).is_none());
    }
    #[test]
    fn mobs_usam_canais_separados_do_jogador() {
        assert_eq!(ataque_mob(2), Som::MobDisparo);
        assert_eq!(ataque_mob(4), Som::MobMagia);
        assert_eq!(ataque_mob(6), Som::MobFlecha);
        for kind in [0, 1, 3, 5] {
            assert_eq!(ataque_mob(kind), Som::MobAtaque);
        }
        assert_ne!(ataque_mob(2), Som::Disparo);
    }
    #[test]
    fn multidao_nao_ocupa_reserva_do_jogador() {
        let externos = vec![(0, 0., false), (1, 0., false)];
        assert!(vaga(&externos, false).is_err());
        assert_eq!(vaga(&externos, true), Ok(None));
        let locais = vec![(0, 0., true); 6];
        assert!(vaga(&locais, false).is_err());
        let mut cheio = locais;
        cheio[3].2 = false;
        assert_eq!(vaga(&cheio, true), Ok(Some(3)));
    }
    #[test]
    fn distancia_preserva_jogador_e_silencia_longe() {
        assert_eq!(ganho(100., true), 1.);
        assert_eq!(ganho(32., false), 0.);
        assert!(ganho(8., false) > ganho(20., false));
    }
}
