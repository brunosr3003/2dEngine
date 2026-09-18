//! Golpes de chefe no chao: o aviso de onde vai bater.
//!
//! O servidor manda a forma e quanto falta pro impacto (`Telegrafico`); aqui
//! se desenha rente ao relevo, estilo MMO, pensado pra ler de CAMERA ALTA em
//! grama, areia, neve ou calcada:
//!
//! * na CARGA: fundo vermelho fraco, preenchimento que cresce ate' a borda com
//!   uma frente clara correndo na ponta, borda dupla (traco escuro por baixo
//!   pra contraste + traco vivo + brilho de dentro), setas andando na direcao
//!   da linha e do cone, e a borda PISCA nos ultimos `ALERTA_S`;
//! * no IMPACTO: clarao da forma inteira, onda de choque saindo da borda e uma
//!   marca de poeira com rachaduras que fica um pouco e some.
//!
//! Quem decide quem tomou e' o servidor — isto e' so' pra dar tempo de sair.

use macroquad::prelude::*;
use shared::bosses::{preenchimento, Forma};
use shared::EntityId;
use std::cell::Cell;
use std::f32::consts::{PI, TAU};

/// Quanto o clarao do impacto dura.
const CLARAO_S: f64 = 0.3;
/// A onda de choque que sai da forma no impacto.
const CHOQUE_S: f64 = 0.45;
/// A marca no chao (poeira e rachadura) que fica depois do golpe.
pub const POEIRA_S: f64 = 1.4;
/// Ultimos segundos da carga em que a borda pisca.
const ALERTA_S: f32 = 0.3;
/// Folga acima do chao (nao briga com o terreno).
const ACIMA: f32 = 0.08;
/// Teto de vertices por malha (a macroquad corta acima de ~10 mil).
const MAX_VERTICES: usize = 3_000;
/// Espessura do contorno.
const BORDA: f32 = 0.14;

#[derive(Debug, Clone, Copy)]
struct Aviso {
    id: u32,
    chefe: EntityId,
    forma: Forma,
    centro: Vec2,
    dir: Vec2,
    carga_s: f32,
    recebido: f64,
    /// Quando o golpe saiu (impacto) ou foi cancelado (some na hora).
    fim: Option<(f64, bool)>,
}

/// Os golpes carregando na AOI.
#[derive(Debug, Default)]
pub struct Telegrafos {
    avisos: Vec<Aviso>,
}

impl Telegrafos {
    #[allow(clippy::too_many_arguments)]
    pub fn comeca(
        &mut self,
        id: u32,
        chefe: EntityId,
        forma: Forma,
        centro: [f32; 2],
        dir: [f32; 2],
        carga_s: f32,
        agora: f64,
    ) {
        self.avisos.retain(|a| a.id != id);
        self.avisos.push(Aviso {
            id,
            chefe,
            forma,
            centro: vec2(centro[0], centro[1]),
            dir: vec2(dir[0], dir[1]).normalize_or(Vec2::X),
            carga_s,
            recebido: agora,
            fim: None,
        });
    }

    /// Fim do golpe. Devolve (chefe, forma, centro, direcao) pra quem precisa
    /// reagir no impacto (animacao, rajadas, tremor).
    pub fn termina(
        &mut self,
        id: u32,
        impacto: bool,
        agora: f64,
    ) -> Option<(EntityId, Forma, Vec2, Vec2)> {
        let a = self.avisos.iter_mut().find(|a| a.id == id)?;
        a.fim = Some((agora, impacto));
        Some((a.chefe, a.forma, a.centro, a.dir))
    }

    pub fn limpa(&mut self) {
        self.avisos.clear();
    }

    /// Tira o que ja' acabou (marca do chao sumiu, cancelado, ou mensagem de
    /// fim perdida muito depois do impacto).
    fn envelhece(&mut self, agora: f64) {
        self.avisos.retain(|a| match a.fim {
            Some((t, true)) => agora - t < POEIRA_S,
            Some((_, false)) => false,
            None => agora - a.recebido < a.carga_s as f64 + 1.5,
        });
    }

    pub fn quantos(&self) -> usize {
        self.avisos.len()
    }

    /// Desenha todos. Vai no passe do mundo (depois do terreno).
    pub fn desenha(&mut self, altura: &dyn Fn(f32, f32) -> f32, agora: f64) {
        self.envelhece(agora);
        for a in &self.avisos {
            let e = estilo_em(a.carga_s, a.recebido, a.fim, agora);
            for (vertices, indices) in formas(&a.forma, a.centro, a.dir, e, altura) {
                draw_mesh(&Mesh {
                    vertices,
                    indices,
                    texture: None,
                });
            }
        }
    }
}

/// O estado visual de um aviso num instante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estilo {
    /// Preenchimento da carga (0..1).
    pub t: f32,
    /// Pisca de alerta no fim da carga (0..1; 0 fora do alerta).
    pub pulso: f32,
    /// Clarao do impacto (1 no impacto, some).
    pub clarao: f32,
    /// Onda de choque (0 no impacto .. 1 = acabou; 1 antes do impacto).
    pub choque: f32,
    /// Marca de poeira (1 no impacto, some).
    pub poeira: f32,
    /// Relogio, pras setas andarem.
    pub tempo: f32,
}

impl Estilo {
    fn depois_do_impacto(&self) -> bool {
        self.clarao > 0.0 || self.choque < 1.0 || self.poeira > 0.0
    }
}

pub fn estilo_em(carga_s: f32, recebido: f64, fim: Option<(f64, bool)>, agora: f64) -> Estilo {
    match fim {
        Some((f, true)) => {
            let d = (agora - f).max(0.0);
            Estilo {
                t: 1.0,
                pulso: 0.0,
                clarao: (1.0 - d / CLARAO_S).clamp(0.0, 1.0) as f32,
                choque: if d < CHOQUE_S {
                    (d / CHOQUE_S) as f32
                } else {
                    1.0
                },
                poeira: (1.0 - d / POEIRA_S).clamp(0.0, 1.0) as f32,
                tempo: agora as f32,
            }
        }
        _ => {
            let decorrido = (agora - recebido) as f32;
            let resta = carga_s - decorrido;
            let pulso = if resta < ALERTA_S {
                0.5 + 0.5 * (agora as f32 * 38.0).sin()
            } else {
                0.0
            };
            Estilo {
                t: preenchimento(decorrido, carga_s),
                pulso,
                clarao: 0.0,
                choque: 1.0,
                poeira: 0.0,
                tempo: agora as f32,
            }
        }
    }
}

/// A malha de uma forma no estilo `e`.
pub fn formas(
    forma: &Forma,
    centro: Vec2,
    dir: Vec2,
    e: Estilo,
    altura: &dyn Fn(f32, f32) -> f32,
) -> Vec<(Vec<Vertex>, Vec<u16>)> {
    let mut m = Malha::default();
    let dir = dir.normalize_or(Vec2::X);
    let ang = dir.y.atan2(dir.x);
    if e.depois_do_impacto() {
        impacto(&mut m, forma, centro, dir, ang, e, altura);
    } else {
        carga(&mut m, forma, centro, dir, ang, e, altura);
    }
    m.fecha()
}

fn a8(x: f32) -> u8 {
    x.clamp(0.0, 255.0) as u8
}

fn carga(
    m: &mut Malha,
    forma: &Forma,
    centro: Vec2,
    dir: Vec2,
    ang: f32,
    e: Estilo,
    altura: &dyn Fn(f32, f32) -> f32,
) {
    let t = e.t;
    let forte = 0.35 + 0.65 * t * t;
    let fundo = [200, 35, 28, a8(35.0 + 55.0 * t)];
    let cheio = [235, 70, 45, a8(80.0 + 90.0 * forte)];
    let frente = [255, 190, 140, a8(150.0 + 80.0 * forte)];
    let sombra = [35, 0, 0, a8(120.0 + 60.0 * forte)];
    let viva = forte.max(e.pulso);
    let borda = [
        255,
        a8(80.0 + 130.0 * e.pulso),
        a8(55.0 + 130.0 * e.pulso),
        a8(175.0 + 80.0 * viva),
    ];
    let brilho = [255, 210, 170, a8(60.0 + 70.0 * forte)];
    let seta = [255, 225, 205, a8(110.0 + 110.0 * forte)];
    let frente_larg = 0.35;
    match *forma {
        Forma::Circulo { raio } => {
            m.setor(centro, ang, PI, 0.0, raio, fundo, altura);
            m.setor(centro, ang, PI, 0.0, raio * t, cheio, altura);
            m.setor(
                centro,
                ang,
                PI,
                (raio * t - frente_larg).max(0.0),
                raio * t,
                frente,
                altura,
            );
            m.setor(
                centro,
                ang,
                PI,
                raio - BORDA * 0.3,
                raio + BORDA * 1.3,
                sombra,
                altura,
            );
            m.setor(centro, ang, PI, raio - BORDA, raio, borda, altura);
            m.setor(
                centro,
                ang,
                PI,
                raio - BORDA * 2.4,
                raio - BORDA * 1.4,
                brilho,
                altura,
            );
        }
        Forma::Anel { interno, externo } => {
            m.setor(centro, ang, PI, interno, externo, fundo, altura);
            let ate = interno + (externo - interno) * t;
            m.setor(centro, ang, PI, interno, ate, cheio, altura);
            m.setor(
                centro,
                ang,
                PI,
                (ate - frente_larg).max(interno),
                ate,
                frente,
                altura,
            );
            m.setor(
                centro,
                ang,
                PI,
                externo - BORDA * 0.3,
                externo + BORDA * 1.3,
                sombra,
                altura,
            );
            m.setor(centro, ang, PI, externo - BORDA, externo, borda, altura);
            m.setor(
                centro,
                ang,
                PI,
                (interno - BORDA * 1.3).max(0.0),
                interno + BORDA * 0.3,
                sombra,
                altura,
            );
            m.setor(centro, ang, PI, interno, interno + BORDA, borda, altura);
            m.setor(
                centro,
                ang,
                PI,
                externo - BORDA * 2.4,
                externo - BORDA * 1.4,
                brilho,
                altura,
            );
        }
        Forma::Cone { raio, abertura } => {
            m.setor(centro, ang, abertura, 0.0, raio, fundo, altura);
            m.setor(centro, ang, abertura, 0.0, raio * t, cheio, altura);
            m.setor(
                centro,
                ang,
                abertura,
                (raio * t - frente_larg).max(0.0),
                raio * t,
                frente,
                altura,
            );
            m.setor(
                centro,
                ang,
                abertura,
                raio - BORDA * 0.3,
                raio + BORDA * 1.3,
                sombra,
                altura,
            );
            m.setor(centro, ang, abertura, raio - BORDA, raio, borda, altura);
            for lado in [-abertura, abertura] {
                let ponta = centro + Vec2::from_angle(ang + lado) * raio;
                m.faixa(centro, ponta, BORDA * 2.6, sombra, altura);
                m.faixa(centro, ponta, BORDA, borda, altura);
            }
            // Setas no eixo, andando pra fora.
            let anda = (e.tempo * 0.8).rem_euclid(0.25);
            for base in [0.3f32, 0.55, 0.8] {
                let r = (base + anda) * raio;
                if r < raio - 0.3 {
                    m.seta(centro + dir * r, dir, (raio * 0.1).max(0.35), seta, altura);
                }
            }
        }
        Forma::Linha {
            comprimento,
            largura,
        } => {
            let fim = centro + dir * comprimento;
            m.retangulo(centro, fim, largura, fundo, altura);
            let ate = comprimento * t;
            m.retangulo(centro, centro + dir * ate, largura, cheio, altura);
            m.retangulo(
                centro + dir * (ate - frente_larg).max(0.0),
                centro + dir * ate,
                largura,
                frente,
                altura,
            );
            let lado = dir.perp() * (largura * 0.5);
            for (a, b) in [
                (centro + lado, fim + lado),
                (centro - lado, fim - lado),
                (fim - lado, fim + lado),
            ] {
                m.faixa(a, b, BORDA * 2.6, sombra, altura);
                m.faixa(a, b, BORDA, borda, altura);
            }
            // Setas ao longo da linha, andando na direcao da investida.
            let passo = 2.6;
            let mut s = 1.2 + (e.tempo * 3.5).rem_euclid(passo);
            while s < comprimento - 0.4 {
                m.seta(
                    centro + dir * s,
                    dir,
                    (largura * 0.3).max(0.35),
                    seta,
                    altura,
                );
                s += passo;
            }
        }
    }
}

fn impacto(
    m: &mut Malha,
    forma: &Forma,
    centro: Vec2,
    dir: Vec2,
    ang: f32,
    e: Estilo,
    altura: &dyn Fn(f32, f32) -> f32,
) {
    // A marca: poeira da forma inteira e rachaduras escuras, sumindo juntas.
    if e.poeira > 0.0 {
        let poeira = [70, 52, 38, a8(120.0 * e.poeira)];
        let racha = [28, 20, 14, a8(175.0 * e.poeira)];
        preenche(m, forma, centro, dir, ang, poeira, altura);
        match *forma {
            Forma::Circulo { raio } | Forma::Cone { raio, .. } => {
                let abre = if let Forma::Cone { abertura, .. } = *forma {
                    abertura
                } else {
                    PI
                };
                for k in 0..7 {
                    let a = ang - abre
                        + 2.0 * abre * (k as f32 + 0.5) / 7.0
                        + ((k * 37) % 11) as f32 * 0.02;
                    let d = Vec2::from_angle(a);
                    let quebra = centro + d * raio * 0.55 + d.perp() * 0.25;
                    m.faixa(centro + d * raio * 0.12, quebra, 0.10, racha, altura);
                    m.faixa(
                        quebra,
                        centro + Vec2::from_angle(a + 0.08) * raio * 0.92,
                        0.07,
                        racha,
                        altura,
                    );
                }
            }
            Forma::Anel { interno, externo } => {
                for k in 0..10 {
                    let d = Vec2::from_angle(k as f32 * TAU / 10.0 + 0.2);
                    m.faixa(
                        centro + d * interno,
                        centro + Vec2::from_angle(k as f32 * TAU / 10.0 + 0.3) * externo,
                        0.09,
                        racha,
                        altura,
                    );
                }
            }
            Forma::Linha {
                comprimento,
                largura,
            } => {
                let lado = dir.perp() * (largura * 0.18);
                let n = (comprimento / 1.6).ceil().max(2.0) as usize;
                for k in 0..n {
                    let (a, b) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
                    let s = if k % 2 == 0 { 1.0 } else { -1.0 };
                    m.faixa(
                        centro + dir * comprimento * a + lado * s,
                        centro + dir * comprimento * b - lado * s,
                        0.09,
                        racha,
                        altura,
                    );
                }
            }
        }
    }
    // O clarao: a forma inteira acende no instante do golpe.
    if e.clarao > 0.0 {
        preenche(
            m,
            forma,
            centro,
            dir,
            ang,
            [255, 235, 200, a8(200.0 * e.clarao)],
            altura,
        );
    }
    // A onda de choque sai da borda e se abre.
    if e.choque < 1.0 {
        let r = forma.alcance() * (0.55 + 0.75 * e.choque);
        let larg = 0.55 * (1.0 - e.choque) + 0.15;
        let abre = if let Forma::Cone { abertura, .. } = *forma {
            abertura
        } else {
            PI
        };
        m.setor(
            centro,
            ang,
            abre,
            (r - larg).max(0.0),
            r,
            [255, 225, 190, a8(220.0 * (1.0 - e.choque))],
            altura,
        );
    }
}

fn preenche(
    m: &mut Malha,
    forma: &Forma,
    centro: Vec2,
    dir: Vec2,
    ang: f32,
    cor: [u8; 4],
    altura: &dyn Fn(f32, f32) -> f32,
) {
    match *forma {
        Forma::Circulo { raio } => m.setor(centro, ang, PI, 0.0, raio, cor, altura),
        Forma::Anel { interno, externo } => m.setor(centro, ang, PI, interno, externo, cor, altura),
        Forma::Cone { raio, abertura } => m.setor(centro, ang, abertura, 0.0, raio, cor, altura),
        Forma::Linha {
            comprimento,
            largura,
        } => m.retangulo(centro, centro + dir * comprimento, largura, cor, altura),
    }
}

#[derive(Default)]
struct Malha {
    feitas: Vec<(Vec<Vertex>, Vec<u16>)>,
    v: Vec<Vertex>,
    i: Vec<u16>,
}

impl Malha {
    fn ponto(&mut self, p: Vec2, cor: [u8; 4], altura: &dyn Fn(f32, f32) -> f32) -> u16 {
        let n = self.v.len() as u16;
        self.v.push(Vertex {
            position: vec3(p.x, altura(p.x, p.y) + ACIMA, p.y),
            uv: Vec2::ZERO,
            color: cor,
            normal: Vec4::ZERO,
        });
        n
    }

    /// Um quadrilatero com as duas faces (o descarte de face de costas esta'
    /// ligado e a camera pode ver o chao dos dois lados numa encosta).
    fn quad(&mut self, q: [Vec2; 4], cor: [u8; 4], altura: &dyn Fn(f32, f32) -> f32) {
        if self.v.len() + 4 > MAX_VERTICES {
            self.feitas
                .push((std::mem::take(&mut self.v), std::mem::take(&mut self.i)));
        }
        let b = [q[0], q[1], q[2], q[3]].map(|p| self.ponto(p, cor, altura));
        self.i.extend_from_slice(&[
            b[0], b[1], b[2], b[0], b[2], b[3], b[0], b[2], b[1], b[0], b[3], b[2],
        ]);
    }

    /// Setor de coroa: de `r0` a `r1`, `abertura` pra cada lado de `ang`.
    #[allow(clippy::too_many_arguments)]
    fn setor(
        &mut self,
        c: Vec2,
        ang: f32,
        abertura: f32,
        r0: f32,
        r1: f32,
        cor: [u8; 4],
        altura: &dyn Fn(f32, f32) -> f32,
    ) {
        if r1 <= r0 + 1e-3 {
            return;
        }
        let passos = ((abertura * 2.0 * r1.max(1.0) / 0.9).ceil() as usize).clamp(6, 48);
        let raios = ((r1 - r0) / 2.5).ceil().max(1.0) as usize;
        for k in 0..raios {
            let (ra, rb) = (
                r0 + (r1 - r0) * k as f32 / raios as f32,
                r0 + (r1 - r0) * (k + 1) as f32 / raios as f32,
            );
            for s in 0..passos {
                let a0 = ang - abertura + 2.0 * abertura * s as f32 / passos as f32;
                let a1 = ang - abertura + 2.0 * abertura * (s + 1) as f32 / passos as f32;
                let p = |a: f32, r: f32| c + Vec2::from_angle(a) * r;
                self.quad([p(a0, ra), p(a1, ra), p(a1, rb), p(a0, rb)], cor, altura);
            }
        }
    }

    /// Retangulo de `a` a `b` com `largura`, em pedacos que seguem o relevo.
    fn retangulo(
        &mut self,
        a: Vec2,
        b: Vec2,
        largura: f32,
        cor: [u8; 4],
        altura: &dyn Fn(f32, f32) -> f32,
    ) {
        let comp = a.distance(b);
        if comp < 1e-3 {
            return;
        }
        let dir = (b - a) / comp;
        let lado = dir.perp() * (largura * 0.5);
        let n = (comp / 1.5).ceil().max(1.0) as usize;
        for k in 0..n {
            let p0 = a + dir * (comp * k as f32 / n as f32);
            let p1 = a + dir * (comp * (k + 1) as f32 / n as f32);
            self.quad([p0 - lado, p1 - lado, p1 + lado, p0 + lado], cor, altura);
        }
    }

    fn faixa(
        &mut self,
        a: Vec2,
        b: Vec2,
        grossura: f32,
        cor: [u8; 4],
        altura: &dyn Fn(f32, f32) -> f32,
    ) {
        self.retangulo(a, b, grossura, cor, altura);
    }

    /// Chevron com a PONTA em `ponta`, apontando pra `dir`.
    fn seta(
        &mut self,
        ponta: Vec2,
        dir: Vec2,
        tam: f32,
        cor: [u8; 4],
        altura: &dyn Fn(f32, f32) -> f32,
    ) {
        let tras = -dir * tam;
        let lado = dir.perp() * tam * 0.8;
        self.faixa(ponta, ponta + tras + lado, 0.13, cor, altura);
        self.faixa(ponta, ponta + tras - lado, 0.13, cor, altura);
    }

    fn fecha(mut self) -> Vec<(Vec<Vertex>, Vec<u16>)> {
        if !self.i.is_empty() {
            self.feitas.push((self.v, self.i));
        }
        self.feitas
    }
}

/// "Fase 2" abaixo de metade da vida (a mesma regra do servidor).
pub fn texto_de_fase(hp: u16, hp_max: u16) -> &'static str {
    if shared::bosses::fase(hp as i32, hp_max as i32) >= 1 {
        "FASE 2"
    } else {
        "FASE 1"
    }
}

/// Chefe vivo, perto e em luta (vida abaixo do maximo): (nome, nivel, hp, max).
pub fn chefe_perto(world: &crate::world::World, eu: Vec2) -> Option<(String, u16, u16, u16)> {
    world
        .ents
        .values()
        .filter(|e| {
            e.meta.tag == shared::EntityTag::Enemy
                && e.state.flags & shared::ent_flags::BOSS != 0
                && e.morte.is_none()
                && e.state.hp > 0
                && e.state.hp < e.meta.hp_max
                && e.render_pos.distance(eu) <= 30.0
        })
        .min_by(|a, b| {
            a.render_pos
                .distance_squared(eu)
                .total_cmp(&b.render_pos.distance_squared(eu))
        })
        .map(|e| {
            (
                e.meta.name.clone().unwrap_or_else(|| "Chefe".into()),
                e.meta.nivel,
                e.state.hp,
                e.meta.hp_max,
            )
        })
}

/// Quanto da barra de "vida perdida" desce por segundo (fracao da vida).
const DESCIDA_DA_PERDIDA: f32 = 0.35;

/// A faixa amarela atras da vida: acompanha na hora quando a vida SOBE e desce
/// devagar quando ela cai — e' o que mostra de quanto foi o golpe.
pub fn vida_perdida(mostrado: f32, alvo: f32, dt: f32) -> f32 {
    if alvo >= mostrado {
        alvo
    } else {
        (mostrado - dt.max(0.0) * DESCIDA_DA_PERDIDA).max(alvo)
    }
}

thread_local! {
    /// (chave do chefe, fracao mostrada, quando).
    static PERDIDA: Cell<(u64, f32, f64)> = const { Cell::new((0, 1.0, 0.0)) };
}

/// Barra de chefe no lugar do painel de alvo, quando o chefe nao e' o alvo.
pub fn desenha_barra_de_chefe(slot: Rect, nome: &str, nivel: u16, hp: u16, hp_max: u16) {
    use crate::hud_estilo as estilo;
    let f = (hp as f32 / hp_max.max(1) as f32).clamp(0.0, 1.0);
    let chave = nome.bytes().fold(nivel as u64 ^ 0x9E37, |h, b| {
        h.wrapping_mul(131).wrapping_add(b as u64)
    });
    let agora = get_time();
    let mostrado = PERDIDA.with(|c| {
        let (k, m, t) = c.get();
        let m = if k != chave {
            f
        } else {
            vida_perdida(m, f, (agora - t) as f32)
        };
        c.set((chave, m, agora));
        m
    });
    let laranja = Color::new(1.0, 0.64, 0.37, 1.0);
    estilo::painel(slot);
    // Moldura de chefe: filete dourado por dentro do painel.
    draw_rectangle_lines(
        slot.x + 3.0,
        slot.y + 3.0,
        slot.w - 6.0,
        slot.h - 6.0,
        1.5,
        Color::new(estilo::OURO.r, estilo::OURO.g, estilo::OURO.b, 0.85),
    );
    desenha_coroa(vec2(slot.x + 22.0, slot.y + 24.0), 8.0);
    estilo::texto(
        slot.x + 40.0,
        slot.y + 18.0,
        &format!("CHEFE · {}", texto_de_fase(hp, hp_max)),
        11,
        laranja,
    );
    estilo::texto(
        slot.x + 40.0,
        slot.y + 36.0,
        &format!("{nome} · Lv {nivel}"),
        15,
        estilo::TEXTO,
    );
    let barra = Rect::new(slot.x + 12.0, slot.y + slot.h - 22.0, slot.w - 24.0, 12.0);
    draw_rectangle(
        barra.x - 1.0,
        barra.y - 1.0,
        barra.w + 2.0,
        barra.h + 2.0,
        Color::new(0.0, 0.0, 0.0, 0.7),
    );
    draw_rectangle(
        barra.x,
        barra.y,
        barra.w,
        barra.h,
        Color::new(0.18, 0.05, 0.05, 0.95),
    );
    // Vida perdida (amarela), depois a vida (vermelha em duas camadas).
    draw_rectangle(
        barra.x,
        barra.y,
        barra.w * mostrado,
        barra.h,
        Color::new(0.96, 0.78, 0.30, 1.0),
    );
    draw_rectangle(
        barra.x,
        barra.y,
        barra.w * f,
        barra.h,
        Color::new(0.72, 0.14, 0.16, 1.0),
    );
    draw_rectangle(
        barra.x,
        barra.y,
        barra.w * f,
        barra.h * 0.4,
        Color::new(0.95, 0.35, 0.30, 0.9),
    );
    // Marca da fase 2 (metade da vida).
    let meio = barra.x + barra.w * 0.5;
    draw_line(
        meio,
        barra.y - 2.0,
        meio,
        barra.y + barra.h + 2.0,
        1.5,
        Color::new(1.0, 1.0, 1.0, 0.55),
    );
    draw_rectangle_lines(
        barra.x,
        barra.y,
        barra.w,
        barra.h,
        1.0,
        Color::new(estilo::OURO.r, estilo::OURO.g, estilo::OURO.b, 0.6),
    );
    estilo::texto_centro(
        barra.x + barra.w * 0.5,
        barra.y + 10.0,
        &format!("{hp} / {hp_max}"),
        10,
        estilo::TEXTO,
    );
}

/// Rotulo de fase por cima do painel de alvo, quando o alvo e' chefe.
pub fn rotulo_de_fase(slot: Rect, hp: u16, hp_max: u16) {
    use crate::hud_estilo as estilo;
    estilo::texto(
        slot.x + slot.w - 150.0,
        slot.y + 16.0,
        texto_de_fase(hp, hp_max),
        10,
        Color::new(1.0, 0.64, 0.37, 1.0),
    );
}

/// Coroa de chefe (mapa, placa e barra).
pub fn desenha_coroa(q: Vec2, tamanho: f32) {
    let ouro = Color::new(1.0, 0.72, 0.25, 1.0);
    let s = tamanho;
    draw_circle(q.x, q.y, s * 1.25, Color::new(0.25, 0.05, 0.05, 0.85));
    let base = q.y + s * 0.45;
    draw_rectangle(q.x - s * 0.7, base - s * 0.25, s * 1.4, s * 0.3, ouro);
    for dx in [-0.6f32, 0.0, 0.6] {
        let topo = q.y - if dx == 0.0 { s * 0.75 } else { s * 0.45 };
        draw_triangle(
            vec2(q.x + (dx - 0.3) * s, base - s * 0.2),
            vec2(q.x + (dx + 0.3) * s, base - s * 0.2),
            vec2(q.x + dx * s, topo),
            ouro,
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn raio_max(m: &[(Vec<Vertex>, Vec<u16>)], c: Vec2) -> f32 {
        m.iter()
            .flat_map(|(v, _)| v.iter())
            .map(|v| vec2(v.position.x, v.position.z).distance(c))
            .fold(0.0, f32::max)
    }

    fn carregando(t: f32) -> Estilo {
        Estilo {
            t,
            pulso: 0.0,
            clarao: 0.0,
            choque: 1.0,
            poeira: 0.0,
            tempo: 0.0,
        }
    }

    const TODAS: [Forma; 4] = [
        Forma::Circulo { raio: 7.0 },
        Forma::Cone {
            raio: 10.0,
            abertura: 0.9,
        },
        Forma::Linha {
            comprimento: 22.0,
            largura: 3.0,
        },
        Forma::Anel {
            interno: 5.0,
            externo: 11.0,
        },
    ];

    #[test]
    fn o_preenchimento_cresce_ate_o_impacto() {
        assert_eq!(preenchimento(0.0, 1.5), 0.0);
        assert!((preenchimento(0.75, 1.5) - 0.5).abs() < 1e-5);
        assert_eq!(preenchimento(9.0, 1.5), 1.0);
        let chao = |_: f32, _: f32| 1.0;
        let c = Vec2::ZERO;
        let meia = formas(
            &Forma::Circulo { raio: 5.0 },
            c,
            Vec2::X,
            carregando(0.5),
            &chao,
        );
        let cheia = formas(
            &Forma::Circulo { raio: 5.0 },
            c,
            Vec2::X,
            carregando(1.0),
            &chao,
        );
        let conta = |m: &[(Vec<Vertex>, Vec<u16>)]| m.iter().map(|(v, _)| v.len()).sum::<usize>();
        assert!(conta(&meia) < conta(&cheia));
        assert!(
            (raio_max(&cheia, c) - 5.0).abs() < 0.2,
            "a borda nao passa muito da forma"
        );
    }

    #[test]
    fn toda_forma_cabe_no_teto_de_indice_e_segue_o_chao() {
        let chao = |x: f32, _: f32| x * 0.1;
        let impacto = Estilo {
            t: 1.0,
            pulso: 0.0,
            clarao: 0.8,
            choque: 0.3,
            poeira: 0.9,
            tempo: 2.0,
        };
        let alerta = Estilo {
            pulso: 1.0,
            tempo: 5.3,
            ..carregando(0.95)
        };
        for f in TODAS {
            for e in [carregando(0.7), alerta, impacto] {
                let m = formas(&f, Vec2::ZERO, Vec2::Y, e, &chao);
                assert!(!m.is_empty(), "{f:?} sem malha");
                for (v, i) in &m {
                    assert!(
                        v.len() <= MAX_VERTICES && i.len() <= 5_000 * 2,
                        "{f:?}: {} vertices",
                        v.len()
                    );
                    assert!(i.iter().all(|k| (*k as usize) < v.len()));
                    for p in v {
                        assert!(
                            (p.position.y - (p.position.x * 0.1 + ACIMA)).abs() < 1e-4,
                            "fora do chao"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_borda_pisca_so_no_fim_da_carga() {
        let longe = estilo_em(2.0, 0.0, None, 1.0);
        assert_eq!(longe.pulso, 0.0);
        let perto = estilo_em(2.0, 0.0, None, 1.85);
        assert!((0.0..=1.0).contains(&perto.pulso));
        let (a, b) = (
            estilo_em(2.0, 0.0, None, 1.80),
            estilo_em(2.0, 0.0, None, 1.84),
        );
        assert_ne!(a.pulso, b.pulso, "o alerta tem que variar no tempo");
    }

    #[test]
    fn impacto_clarao_choque_e_poeira_na_ordem() {
        let e0 = estilo_em(1.0, 0.0, Some((1.0, true)), 1.1);
        assert!(e0.clarao > 0.0 && e0.choque < 1.0 && e0.poeira > 0.9);
        let e1 = estilo_em(1.0, 0.0, Some((1.0, true)), 1.0 + 0.6);
        assert_eq!(e1.clarao, 0.0);
        assert_eq!(e1.choque, 1.0, "a onda ja' passou");
        assert!(e1.poeira > 0.0, "a marca ainda esta' no chao");
        let e2 = estilo_em(1.0, 0.0, Some((1.0, true)), 1.0 + POEIRA_S);
        assert_eq!(e2.poeira, 0.0);
        // A onda sai DA forma e passa da borda.
        let chao = |_: f32, _: f32| 0.0;
        let tarde = Estilo {
            t: 1.0,
            pulso: 0.0,
            clarao: 0.0,
            choque: 0.9,
            poeira: 0.0,
            tempo: 0.0,
        };
        let m = formas(
            &Forma::Circulo { raio: 4.0 },
            Vec2::ZERO,
            Vec2::X,
            tarde,
            &chao,
        );
        assert!(raio_max(&m, Vec2::ZERO) > 4.0 * 1.1, "a onda nao se abriu");
    }

    #[test]
    fn aviso_some_depois_da_marca_ou_cancelado() {
        let mut t = Telegrafos::default();
        let chefe = EntityId(77);
        t.comeca(
            1,
            chefe,
            Forma::Circulo { raio: 3.0 },
            [0.0, 0.0],
            [1.0, 0.0],
            1.0,
            0.0,
        );
        t.comeca(
            2,
            chefe,
            Forma::Circulo { raio: 3.0 },
            [0.0, 0.0],
            [1.0, 0.0],
            1.0,
            0.0,
        );
        assert_eq!(
            t.termina(1, true, 1.0).map(|x| x.0),
            Some(chefe),
            "o fim diz de quem era o golpe"
        );
        t.termina(2, false, 0.5);
        t.envelhece(1.1);
        assert_eq!(
            t.quantos(),
            1,
            "o impacto ainda marca o chao, o cancelado some"
        );
        t.envelhece(1.0 + POEIRA_S - 0.05);
        assert_eq!(t.quantos(), 1);
        t.envelhece(1.0 + POEIRA_S + 0.05);
        assert_eq!(t.quantos(), 0);
        assert!(t.termina(9, true, 1.0).is_none());
        assert_eq!(texto_de_fase(40, 100), "FASE 2");
        assert_eq!(texto_de_fase(60, 100), "FASE 1");
    }

    #[test]
    fn vida_perdida_desce_devagar_e_sobe_na_hora() {
        assert_eq!(vida_perdida(0.8, 0.9, 0.1), 0.9, "curou: acompanha na hora");
        let m = vida_perdida(0.8, 0.5, 0.1);
        assert!(m < 0.8 && m > 0.5, "caiu: desce devagar ({m})");
        assert_eq!(vida_perdida(0.52, 0.5, 1.0), 0.5, "nao passa da vida");
    }
}
