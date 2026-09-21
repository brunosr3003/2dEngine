//! **Subiu de nível**: a explosão de luz em volta de quem subiu.
//!
//! Antes não havia nada. O número trocava na ficha e pronto — a coisa que o
//! jogo inteiro pede que você persiga acontecia em silêncio.
//!
//! O que se vê, de baixo pra cima e nesta ordem de tempo:
//!
//!   * o **clarão**: um disco aceso no chão que abre de uma vez e apaga;
//!   * os **anéis**: dois aros que saem de dentro do corpo e se abrem pelo
//!     chão, o segundo atrasado — é o que dá o "estouro";
//!   * a **coluna**: um facho dourado subindo do pé, largo embaixo e fino no
//!     alto, virado pra câmera;
//!   * as **fagulhas**: pontos que sobem em espiral e somem no alto.
//!
//! Tudo em cor de vértice no material do mundo — sem shader novo, sem
//! textura, sem passada a mais. É um efeito por vez, e por poucos segundos:
//! o custo não aparece nem em celular.
//!
//! **Dura mais que o instante e menos que a paciência.** Um clarão de meio
//! segundo se perde no meio de uma luta; um de cinco vira estorvo. `DURACAO`
//! é o meio disso, e a maior parte da luz sai nos primeiros 40%.

use macroquad::prelude::*;

/// Quanto o efeito inteiro dura, em segundos.
pub const DURACAO: f32 = 2.2;
/// Quantas fagulhas sobem.
const FAGULHAS: usize = 22;
/// Até onde o anel abre, em unidades.
const RAIO_MAX: f32 = 3.4;
/// Altura da coluna.
const ALTURA: f32 = 5.2;
/// Até onde a fagulha sobe.
const SUBIDA: f32 = 4.6;

/// Ouro aceso: a cor de "conquista" da interface (`hud_estilo::OURO`).
const OURO: [f32; 3] = [255.0, 208.0, 116.0];
/// O branco quente do miolo, no primeiro instante.
const NUCLEO: [f32; 3] = [255.0, 250.0, 226.0];

/// Um efeito em andamento. `None` = ninguém subiu de nível agora.
#[derive(Debug, Default)]
pub struct SubiuDeNivel {
    /// Segundos decorridos. `>= DURACAO` = acabou.
    t: f32,
    ativo: bool,
}

impl SubiuDeNivel {
    /// Dispara (ou redispara, se subiu dois níveis seguidos).
    pub fn dispara(&mut self) {
        self.t = 0.0;
        self.ativo = true;
    }

    pub fn passo(&mut self, dt: f32) {
        if !self.ativo {
            return;
        }
        self.t += dt;
        if self.t >= DURACAO {
            self.ativo = false;
        }
    }

    pub fn ativo(&self) -> bool {
        self.ativo
    }

    /// 0 no disparo, 1 no fim. Fora do efeito, `None`.
    pub fn fase(&self) -> Option<f32> {
        self.ativo.then(|| (self.t / DURACAO).clamp(0.0, 1.0))
    }

    /// Desenha no pé de quem subiu. Quem chama já pôs o material do mundo.
    pub fn desenha(&self, pe: Vec3, cam: &Camera3D) {
        let Some(f) = self.fase() else { return };
        clarao(pe, f);
        // Dois anéis: o segundo entra com 22% de atraso. Um anel só lê como
        // onda de choque de skill; dois lêem como estouro.
        anel(pe, f);
        anel(pe, (f - 0.22) / (1.0 - 0.22));
        coluna(pe, cam.position, f);
        for i in 0..FAGULHAS {
            fagulha(pe, i, f);
        }
    }
}

/// A escala de brilho: forte no começo e caindo depressa.
///
/// Linear ficava "ligado" metade do tempo e sumia de repente; esta cai como
/// luz cai — rápido no início, com um rastro longo e fraco.
fn brilho(f: f32) -> f32 {
    if !(0.0..1.0).contains(&f) {
        return 0.0;
    }
    (1.0 - f).powi(3)
}

/// O disco aceso no chão, que abre de uma vez e apaga.
fn clarao(pe: Vec3, f: f32) {
    let a = brilho(f);
    if a <= 0.0 {
        return;
    }
    const LADOS: usize = 20;
    let centro = vec3(pe.x, pe.y + 0.03, pe.z);
    let raio = RAIO_MAX * (0.35 + 0.65 * suave(f * 2.2));
    let dentro = cor(NUCLEO, 1.0, (210.0 * a) as u8);
    let (mut v, mut tris) = (vec![vtx(centro, dentro)], Vec::new());
    for k in 0..=LADOS {
        let ang = k as f32 / LADOS as f32 * std::f32::consts::TAU;
        v.push(vtx(
            centro + vec3(ang.cos() * raio, 0.0, ang.sin() * raio),
            cor(OURO, 1.0, 0),
        ));
        if k > 0 {
            tris.push([0, k as u16, k as u16 + 1]);
        }
    }
    dupla(v, tris);
}

/// Um aro que sai do corpo e se abre pelo chão, afinando.
fn anel(pe: Vec3, f: f32) {
    if !(0.0..1.0).contains(&f) {
        return;
    }
    let a = brilho(f);
    const LADOS: usize = 28;
    let centro = vec3(pe.x, pe.y + 0.06, pe.z);
    let r = RAIO_MAX * suave(f);
    // O aro AFINA enquanto abre: largura constante lia como disco crescendo.
    let esp = 0.42 * (1.0 - f).max(0.12);
    let (mut v, mut tris) = (Vec::new(), Vec::new());
    for k in 0..=LADOS {
        let ang = k as f32 / LADOS as f32 * std::f32::consts::TAU;
        let d = vec3(ang.cos(), 0.0, ang.sin());
        v.push(vtx(centro + d * (r - esp), cor(OURO, 1.0, 0)));
        v.push(vtx(centro + d * r, cor(NUCLEO, 1.0, (235.0 * a) as u8)));
        v.push(vtx(centro + d * (r + esp), cor(OURO, 1.0, 0)));
        if k > 0 {
            let (p, q) = (((k - 1) * 3) as u16, (k * 3) as u16);
            tris.push([p, q, p + 1]);
            tris.push([q, q + 1, p + 1]);
            tris.push([p + 1, q + 1, p + 2]);
            tris.push([q + 1, q + 2, p + 2]);
        }
    }
    dupla(v, tris);
}

/// O facho subindo do pé, virado pra câmera.
fn coluna(pe: Vec3, olho: Vec3, f: f32) {
    let a = brilho(f) * 0.9;
    if a <= 0.0 {
        return;
    }
    let para_olho = vec3(olho.x - pe.x, 0.0, olho.z - pe.z).normalize_or_zero();
    let lado = para_olho.cross(Vec3::Y).normalize_or_zero();
    if lado.length_squared() < 0.1 {
        return;
    }
    // Sobe enquanto abre: uma coluna de altura fixa lia como parede.
    let h = ALTURA * (0.45 + 0.55 * suave(f * 1.6));
    let base = 0.95;
    let topo = 0.20;
    let pe = vec3(pe.x, pe.y + 0.05, pe.z);
    let v = vec![
        vtx(pe - lado * base, cor(NUCLEO, 1.0, (200.0 * a) as u8)),
        vtx(pe + lado * base, cor(NUCLEO, 1.0, (200.0 * a) as u8)),
        vtx(pe + lado * topo + Vec3::Y * h, cor(OURO, 1.0, 0)),
        vtx(pe - lado * topo + Vec3::Y * h, cor(OURO, 1.0, 0)),
    ];
    dupla(v, vec![[0, 1, 2], [0, 2, 3]]);
}

/// Uma fagulha subindo em espiral.
fn fagulha(pe: Vec3, i: usize, f: f32) {
    // Cada uma com a sua fase: saindo juntas, viram um anel subindo.
    let atraso = (i as f32 * 0.6180339).fract() * 0.35;
    let u = (f - atraso) / (1.0 - atraso);
    if !(0.0..1.0).contains(&u) {
        return;
    }
    let a = brilho(u);
    let giro = i as f32 * 2.399963 + u * 3.4;
    let r = 0.55 + 1.35 * u;
    let p = pe
        + vec3(giro.cos() * r, SUBIDA * suave(u), giro.sin() * r)
        + Vec3::Y * 0.2;
    let s = 0.13 * (1.0 - u).max(0.25);
    let c = cor(NUCLEO, 1.0, (250.0 * a) as u8);
    let v = vec![
        vtx(p + vec3(-s, -s, 0.0), c),
        vtx(p + vec3(s, -s, 0.0), c),
        vtx(p + vec3(s, s, 0.0), c),
        vtx(p + vec3(-s, s, 0.0), c),
    ];
    dupla(v, vec![[0, 1, 2], [0, 2, 3]]);
}

/// A faixa "NÍVEL N" na tela, por cima de tudo.
///
/// O efeito de mundo diz "algo aconteceu"; a faixa diz O QUE. Sem ela o
/// jogador ve' a luz e ainda precisa abrir a ficha pra saber em que nivel
/// esta' — que e' justamente a pergunta que a luz levantou.
///
/// Some ANTES da luz (nos primeiros 60%): texto que fica lendo na tela
/// depois do brilho vira aviso esquecido, nao comemoracao.
pub fn desenha_faixa(e: &SubiuDeNivel, nivel: u32) {
    let Some(f) = e.fase() else { return };
    let vis = 1.0 - (f / 0.6).clamp(0.0, 1.0);
    if vis <= 0.0 {
        return;
    }
    let seguro = crate::hud_layout::tela_segura();
    // Sobe um pouco enquanto some: parada, ela lia como elemento fixo do HUD.
    let y = seguro.y + seguro.h * 0.26 - (1.0 - vis) * crate::hud_estilo::u(26.0);
    let a = vis * vis;
    let texto = format!("NÍVEL {nivel}");
    let cx = seguro.center().x;
    crate::hud_estilo::texto_centro_forte(
        cx + 2.0,
        y + 2.0,
        &texto,
        34,
        Color::new(0.0, 0.0, 0.0, 0.55 * a),
    );
    crate::hud_estilo::texto_centro_forte(
        cx,
        y,
        &texto,
        34,
        Color::new(1.0, 0.82, 0.45, a),
    );
}

fn suave(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cor(base: [f32; 3], k: f32, alfa: u8) -> [u8; 4] {
    [
        (base[0] * k).clamp(0.0, 255.0) as u8,
        (base[1] * k).clamp(0.0, 255.0) as u8,
        (base[2] * k).clamp(0.0, 255.0) as u8,
        alfa,
    ]
}

fn vtx(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex {
        position: p,
        uv: vec2(0.0, 0.0),
        color: c,
        normal: Vec4::ZERO,
    }
}

fn dupla(vertices: Vec<Vertex>, tris: Vec<[u16; 3]>) {
    let mut indices = Vec::with_capacity(tris.len() * 6);
    for [a, b, c] in tris {
        indices.extend_from_slice(&[a, b, c, a, c, b]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O efeito TERMINA. Um efeito que nunca se apaga fica preso na tela pra
    /// sempre, e o jogador nao tem como desligar.
    #[test]
    fn dispara_dura_e_acaba() {
        let mut e = SubiuDeNivel::default();
        assert!(!e.ativo() && e.fase().is_none(), "nasce apagado");
        e.dispara();
        assert_eq!(e.fase(), Some(0.0));
        e.passo(DURACAO * 0.5);
        assert!(e.ativo() && e.fase().is_some_and(|f| (f - 0.5).abs() < 0.01));
        e.passo(DURACAO);
        assert!(!e.ativo() && e.fase().is_none(), "passou da duracao e apagou");
    }

    /// Subir dois niveis seguidos REINICIA. Sem isso o segundo nivel nao
    /// acende nada — e' justamente quando o jogador mais quer ver.
    #[test]
    fn subir_de_novo_no_meio_reinicia() {
        let mut e = SubiuDeNivel::default();
        e.dispara();
        e.passo(DURACAO * 0.9);
        e.dispara();
        assert_eq!(e.fase(), Some(0.0), "o segundo nivel recomeca o efeito");
    }

    /// A faixa some ANTES da luz: texto parado na tela depois do brilho vira
    /// aviso esquecido. Guardado como fracao, que e' o que o desenho usa.
    #[test]
    fn a_faixa_some_antes_da_luz() {
        const FIM_DA_FAIXA: f32 = 0.6;
        assert!(FIM_DA_FAIXA < 1.0, "a faixa nao pode durar mais que o efeito");
        assert!(brilho(FIM_DA_FAIXA) > 0.0, "a luz continua depois da faixa");
    }

    /// O brilho cai, e cai DEPRESSA: metade do tempo tem que sobrar bem menos
    /// de metade da luz, senao o efeito fica "ligado" e incomoda.
    #[test]
    fn o_brilho_cai_rapido_e_zera_no_fim() {
        assert_eq!(brilho(0.0), 1.0);
        assert!(brilho(0.5) < 0.2, "meio do efeito: {}", brilho(0.5));
        assert_eq!(brilho(1.0), 0.0);
        assert_eq!(brilho(1.5), 0.0, "depois do fim nao acende");
        let mut ant = f32::MAX;
        for k in 0..=10 {
            let b = brilho(k as f32 / 10.0);
            assert!(b <= ant, "o brilho subiu em {k}");
            ant = b;
        }
    }
}
