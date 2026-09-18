//! Lascas e faiscas da coleta: a rajada no instante em que a ferramenta bate.
//!
//! O gesto de coleta vem do fio (`acao`), entao todo mundo que ve o golpe ve
//! a rajada: e' disparada no desenho do personagem, no quadro em que a fase
//! do golpe CRUZA o impacto (`rig::fase_do_impacto`). Machado solta lascas de
//! madeira; picareta solta faiscas curtas e po'/pedrinhas na cor da pedra.
//!
//! Pool FIXO: nada aloca por quadro. Rajada nova reaproveita a vaga mais
//! antiga quando o pool esta' cheio — melhor sumir uma lasca velha que crescer.

use macroquad::prelude::*;
use std::cell::RefCell;

/// Vagas do pool. Uma rajada de coleta usa 8–10 e uma de golpe de chefe 7:
/// cabe uma dezena de golpes e a aura dos chefes no ar.
pub const TAMANHO_DO_POOL: usize = 256;
/// Quanto dura uma lasca/pedrinha.
const VIDA_S: f32 = 0.55;
/// Quanto dura uma faisca.
const VIDA_DA_FAISCA_S: f32 = 0.22;
/// Gravidade das lascas, em unidades/s². Mais que a real: o tamanho do
/// boneco na tela pede queda rapida, senao flutua.
const GRAVIDADE: f32 = 9.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Lasca {
    pub pos: Vec3,
    pub vel: Vec3,
    pub idade: f32,
    /// 0 = vaga livre.
    pub vida: f32,
    pub cor: [u8; 3],
    pub tam: f32,
    /// Gravidade desta particula (u/s²). Negativa sobe (aura de chefe).
    pub grav: f32,
}

impl Lasca {
    pub fn viva(&self) -> bool {
        self.idade < self.vida
    }
}

pub struct Lascas {
    pub pool: [Lasca; TAMANHO_DO_POOL],
    prox: usize,
}

impl Default for Lascas {
    fn default() -> Self {
        Self {
            pool: [Lasca::default(); TAMANHO_DO_POOL],
            prox: 0,
        }
    }
}

/// Cor da lasca pelo tipo da coleta: 0 madeira; 1..4 a cor da pedra
/// (`shared::items::tier_color_hex`, a mesma do icone e da cabeca da picareta).
pub fn cor_do_tipo(tipo: u8) -> [u8; 3] {
    if tipo == 0 {
        return [122, 82, 46];
    }
    let h = shared::items::tier_color_hex(tipo.min(4)).trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128);
    [c(0), c(2), c(4)]
}

/// A fase do golpe passou pelo impacto entre o quadro anterior e este? Conta
/// a volta do relogio (fim do golpe → comeco do proximo).
pub fn cruzou(antes: f32, agora: f32, impacto: f32) -> bool {
    if agora >= antes {
        antes < impacto && agora >= impacto
    } else {
        antes < impacto || agora >= impacto
    }
}

impl Lascas {
    /// Uma rajada em `pos`. `semente` so' varia o desenho entre golpes.
    pub fn emite(&mut self, pos: Vec3, tipo: u8, semente: u32) {
        let mut s = semente.wrapping_mul(2_654_435_761) | 1;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s & 0xFFFF) as f32 / 65535.0
        };
        let base = cor_do_tipo(tipo);
        let n = if tipo == 0 { 8 } else { 10 };
        for k in 0..n {
            let ang = rnd() * std::f32::consts::TAU;
            // Na picareta as quatro primeiras sao faiscas: claras, rapidas, curtas.
            let faisca = tipo != 0 && k < 4;
            let lado = 0.5 + rnd() * 1.1;
            let sobe = 1.3 + rnd() * 1.6;
            let forca = if faisca { 1.7 } else { 1.0 };
            let varia = 0.8 + rnd() * 0.35;
            let cor = if faisca {
                [255, 238, 176]
            } else {
                [
                    (base[0] as f32 * varia).min(255.0) as u8,
                    (base[1] as f32 * varia).min(255.0) as u8,
                    (base[2] as f32 * varia).min(255.0) as u8,
                ]
            };
            self.pool[self.prox] = Lasca {
                pos,
                vel: vec3(ang.cos() * lado, sobe, ang.sin() * lado) * forca,
                idade: 0.0,
                vida: if faisca {
                    VIDA_DA_FAISCA_S
                } else {
                    VIDA_S * (0.8 + rnd() * 0.4)
                },
                cor,
                tam: if faisca { 0.022 } else { 0.04 + rnd() * 0.03 },
                grav: GRAVIDADE,
            };
            self.prox = (self.prox + 1) % TAMANHO_DO_POOL;
        }
    }

    fn poe(&mut self, l: Lasca) {
        self.pool[self.prox] = l;
        self.prox = (self.prox + 1) % TAMANHO_DO_POOL;
    }

    /// Rajada de golpe de chefe: pedacos maiores na cor do elemento, saindo
    /// pra cima e pra fora, e um par de brilhos claros.
    pub fn explosao(&mut self, pos: Vec3, cor: [u8; 3], semente: u32) {
        let mut s = semente.wrapping_mul(2_654_435_761) | 1;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s & 0xFFFF) as f32 / 65535.0
        };
        for k in 0..7 {
            let ang = rnd() * std::f32::consts::TAU;
            let brilho = k < 2;
            let lado = 1.2 + rnd() * 2.2;
            let varia = 0.8 + rnd() * 0.4;
            self.poe(Lasca {
                pos,
                vel: vec3(ang.cos() * lado, 2.5 + rnd() * 3.0, ang.sin() * lado),
                idade: 0.0,
                vida: if brilho { 0.3 } else { 0.75 + rnd() * 0.35 },
                cor: if brilho {
                    [255, 245, 220]
                } else {
                    [
                        (cor[0] as f32 * varia).min(255.0) as u8,
                        (cor[1] as f32 * varia).min(255.0) as u8,
                        (cor[2] as f32 * varia).min(255.0) as u8,
                    ]
                },
                tam: if brilho { 0.09 } else { 0.08 + rnd() * 0.07 },
                grav: GRAVIDADE,
            });
        }
    }

    /// Uma faisca de aura: sobe devagar em volta do chefe e some.
    pub fn faisca_de_aura(&mut self, pos: Vec3, cor: [u8; 3], semente: u32) {
        let lado = ((semente % 100) as f32 * 0.01 - 0.5) * 0.3;
        self.poe(Lasca {
            pos,
            vel: vec3(lado, 0.5, -lado),
            idade: 0.0,
            vida: 1.1,
            cor,
            tam: 0.05,
            grav: -0.4,
        });
    }

    pub fn avanca(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        for l in self.pool.iter_mut().filter(|l| l.viva()) {
            l.idade += dt;
            l.vel.y -= l.grav * dt;
            l.pos += l.vel * dt;
        }
    }

    pub fn vivas(&self) -> usize {
        self.pool.iter().filter(|l| l.viva()).count()
    }

    pub fn desenha(&self) {
        for l in self.pool.iter().filter(|l| l.viva()) {
            let u = (l.idade / l.vida).clamp(0.0, 1.0);
            let a = ((1.0 - u * u) * 255.0) as u8;
            let tam = l.tam * (1.0 - 0.5 * u);
            draw_cube(
                l.pos,
                Vec3::splat(tam),
                None,
                Color::from_rgba(l.cor[0], l.cor[1], l.cor[2], a),
            );
        }
    }
}

thread_local! {
    static LASCAS: RefCell<Lascas> = RefCell::new(Lascas::default());
}

/// A rajada de um golpe (chamado do desenho do personagem).
pub fn impacto(pos: Vec3, tipo: u8, semente: u32) {
    LASCAS.with(|l| l.borrow_mut().emite(pos, tipo, semente));
}

/// A rajada de um golpe de chefe (no impacto e na queda dele).
pub fn explosao(pos: Vec3, cor: [u8; 3], semente: u32) {
    LASCAS.with(|l| l.borrow_mut().explosao(pos, cor, semente));
}

/// Uma faisca da aura de chefe.
pub fn aura(pos: Vec3, cor: [u8; 3], semente: u32) {
    LASCAS.with(|l| l.borrow_mut().faisca_de_aura(pos, cor, semente));
}

/// Uma vez por quadro, no passe do mundo.
pub fn avanca_e_desenha(dt: f32) {
    LASCAS.with(|l| {
        let mut l = l.borrow_mut();
        l.avanca(dt);
        l.desenha();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uma_rajada_por_golpe_so_no_impacto() {
        let impacto = crate::rig::fase_do_impacto(1);
        assert!(cruzou(0.50, 0.60, 0.58));
        assert!(!cruzou(0.60, 0.70, 0.58), "depois do impacto nao repete");
        assert!(!cruzou(0.10, 0.20, 0.58), "antes do impacto nao dispara");
        assert!(
            cruzou(0.50, 0.02, 0.58),
            "o relogio virou passando pelo impacto"
        );
        assert!(
            !cruzou(0.90, 0.05, 0.58),
            "virar sem passar pelo impacto nao dispara"
        );
        // Tres golpes a 60 quadros por segundo: tres rajadas.
        let (mut ant, mut n, mut t) = (0.0f32, 0, 0.0f32);
        while t < 3.0 * crate::rig::PERIODO_DA_COLETA - 0.01 {
            t += 1.0 / 60.0;
            let u = (t / crate::rig::PERIODO_DA_COLETA).fract();
            if cruzou(ant, u, impacto) {
                n += 1;
            }
            ant = u;
        }
        assert_eq!(n, 3);
    }

    #[test]
    fn cor_pelo_tipo() {
        let madeira = cor_do_tipo(0);
        assert!(madeira[0] > madeira[2], "madeira e' marrom");
        let cores: Vec<[u8; 3]> = (1..=4).map(cor_do_tipo).collect();
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(
                    cores[i],
                    cores[j],
                    "tier {} e {} com a mesma cor",
                    i + 1,
                    j + 1
                );
            }
        }
        // O po' da rajada sai na cor do tier (as faiscas sao claras e a parte).
        let mut l = Lascas::default();
        l.emite(Vec3::ZERO, 2, 7);
        let base = cor_do_tipo(2);
        assert!(l
            .pool
            .iter()
            .filter(|x| x.viva() && x.cor != [255, 238, 176])
            .all(|x| { (0..3).all(|k| (x.cor[k] as i32 - (base[k] as f32 * 0.8) as i32) >= -1) }));
    }

    #[test]
    fn rajada_de_chefe_cabe_no_pool_e_aura_sobe() {
        let mut l = Lascas::default();
        for k in 0..200 {
            l.explosao(Vec3::ZERO, [255, 140, 40], k);
            l.faisca_de_aura(Vec3::ZERO, [170, 160, 255], k);
        }
        assert_eq!(l.pool.len(), TAMANHO_DO_POOL);
        let mut a = Lascas::default();
        a.faisca_de_aura(Vec3::ZERO, [1, 2, 3], 5);
        for _ in 0..10 {
            a.avanca(0.05);
        }
        assert!(
            a.pool.iter().filter(|x| x.viva()).all(|x| x.pos.y > 0.0),
            "aura sobe"
        );
        let mut e = Lascas::default();
        e.explosao(Vec3::ZERO, [10, 20, 30], 9);
        assert_eq!(e.vivas(), 7);
        for _ in 0..40 {
            e.avanca(0.05);
        }
        assert_eq!(e.vivas(), 0, "a rajada some");
    }

    #[test]
    fn o_pool_nao_cresce_e_as_lascas_caem_e_somem() {
        let mut l = Lascas::default();
        for k in 0..500 {
            l.emite(Vec3::ZERO, (k % 5) as u8, k);
        }
        assert_eq!(l.pool.len(), TAMANHO_DO_POOL);
        assert!(l.vivas() <= TAMANHO_DO_POOL);
        let mut r = Lascas::default();
        r.emite(Vec3::ZERO, 0, 3);
        assert_eq!(r.vivas(), 8, "machado: 8 lascas");
        for _ in 0..6 {
            r.avanca(0.05);
        }
        assert!(
            r.pool
                .iter()
                .filter(|x| x.viva())
                .all(|x| x.vel.y < 1.3 + 1.6),
            "gravidade puxa"
        );
        for _ in 0..40 {
            r.avanca(0.05);
        }
        assert_eq!(r.vivas(), 0, "tudo some");
    }
}
