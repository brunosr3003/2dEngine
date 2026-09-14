//! O tracejado no chao: pra onde o personagem esta' andando.
//!
//! A rota e' do SERVIDOR (ele roda o A*). Ele manda os pontos quando calcula
//! ou refaz e uma rota vazia quando ela acaba; aqui so' se desenha. Os tracos
//! andam na direcao do movimento e seguem o relevo, e o destino ganha um anel
//! que pulsa.
//!
//! A fase dos tracos e' contada a partir do FIM do caminho: o jogador anda e
//! consome o comeco, e os tracos ficam parados no chao em vez de escorregar
//! junto com ele. Rota refeita quase igual (seguir alvo pede rota a cada
//! 0,35 s) nao reinicia nada, porque nao ha' estado de animacao — so' o
//! relogio.

use macroquad::prelude::*;

/// Mesmo "chegou" do seguidor do servidor.
const CHEGOU: f32 = 0.6;
/// Ate' onde, ao longo do caminho, se desenha.
const ALCANCE: f32 = 120.0;
/// Uma amostra de relevo a cada tanto.
const PASSO: f32 = 0.5;
const TRACO: f32 = 0.7;
const VAO: f32 = 0.45;
const LARGURA: f32 = 0.16;
/// Acima do chao, pra nao brigar com ele.
const ELEVACAO: f32 = 0.06;
/// Quanto os tracos andam por segundo.
const VELOCIDADE: f32 = 1.4;
/// Mesmo teto do terreno: 800 quads cabem nos 5.000 indices da macroquad.
const MAX_QUADS: usize = 800;

const CLARO: [u8; 4] = [250, 244, 214, 255];
const SOMBRA: [u8; 4] = [24, 28, 34, 150];
const FRACO: [u8; 4] = [236, 226, 180, 150];

/// A rota do servidor, menos o que o corpo ja' alcancou.
#[derive(Default)]
pub struct Rastro {
    pontos: Vec<Vec2>,
    destino: Option<Vec2>,
}

impl Rastro {
    pub fn define(&mut self, pontos: Vec<Vec2>, destino: Vec2) {
        if pontos.is_empty() {
            self.limpa();
            return;
        }
        self.pontos = pontos;
        self.destino = Some(destino);
    }

    pub fn limpa(&mut self) {
        self.pontos.clear();
        self.destino = None;
    }

    pub fn ativo(&self) -> bool {
        !self.pontos.is_empty()
    }

    pub fn destino(&self) -> Option<Vec2> {
        self.destino.filter(|_| self.ativo())
    }

    /// Descarta os pontos que o corpo ja' alcancou — ou ja' PASSOU: cortando a
    /// quina ele nunca chega a 0,6 do ponto, e o tracejado voltaria pra tras.
    pub fn acompanha(&mut self, eu: Vec2) {
        loop {
            let Some(&p0) = self.pontos.first() else { break };
            let passou = self.pontos.get(1).is_some_and(|&p1| {
                (eu - p0).dot(p1 - p0) > 0.0 && eu.distance(p1) < p0.distance(p1)
            });
            if p0.distance(eu) <= CHEGOU || passou {
                self.pontos.remove(0);
            } else {
                break;
            }
        }
        if self.pontos.is_empty() {
            self.destino = None;
        }
    }

    /// Do personagem pelos pontos que faltam.
    pub fn caminho(&self, eu: Vec2) -> Vec<Vec2> {
        if !self.ativo() {
            return Vec::new();
        }
        let mut c = Vec::with_capacity(self.pontos.len() + 1);
        c.push(eu);
        c.extend_from_slice(&self.pontos);
        c
    }
}

/// Pontos 3D ao longo do caminho, rentes ao relevo, ate' `alcance`. Cada um
/// com o quanto ja' se andou (no plano) desde o comeco.
pub fn amostrar(caminho: &[Vec2], altura: &dyn Fn(f32, f32) -> f32, alcance: f32) -> Vec<(Vec3, f32)> {
    let mut saida: Vec<(Vec3, f32)> = Vec::new();
    let mut andado = 0.0;
    for par in caminho.windows(2) {
        let (a, b) = (par[0], par[1]);
        let comp = a.distance(b);
        if comp < 1e-4 {
            continue;
        }
        let n = (comp / PASSO).ceil().max(1.0) as usize;
        if saida.is_empty() {
            saida.push((vec3(a.x, altura(a.x, a.y) + ELEVACAO, a.y), 0.0));
        }
        for k in 1..=n {
            let p = a.lerp(b, k as f32 / n as f32);
            andado += comp / n as f32;
            saida.push((vec3(p.x, altura(p.x, p.y) + ELEVACAO, p.y), andado));
            if andado >= alcance {
                return saida;
            }
        }
    }
    saida
}

/// Comprimento do caminho no plano.
fn comprimento(caminho: &[Vec2]) -> f32 {
    caminho.windows(2).map(|p| p[0].distance(p[1])).sum()
}

/// Quanto falta andar, PELO CAMINHO: a rota que o servidor mandou e, se a
/// viagem vai alem dela, a reta do fim da rota ate' o destino final. Em reta
/// do personagem ao destino mentiria toda vez que a rota contorna um morro.
pub fn restante(r: &Rastro, eu: Vec2, destino_final: Option<Vec2>) -> Option<f32> {
    if !r.ativo() && destino_final.is_none() {
        return None;
    }
    let (mut total, fim) = if r.ativo() {
        let c = r.caminho(eu);
        (comprimento(&c), *c.last()?)
    } else {
        (0.0, eu)
    };
    if let Some(f) = destino_final {
        if f.distance(fim) > 0.5 {
            total += fim.distance(f);
        }
    }
    Some(total)
}

/// "13 m", ou "1,5 km" a partir de mil. Arredonda pra CIMA: chegar a zero so'
/// quando chegou de verdade.
pub fn formata_distancia(m: f32) -> String {
    if m >= 1000.0 {
        format!("{:.1} km", m / 1000.0).replace('.', ",")
    } else {
        format!("{} m", m.ceil().max(1.0) as i32)
    }
}

/// O contador sobre o destino: plaquinha com quanto falta. Vai no passe 2D,
/// depois do mundo.
pub fn desenha_distancia(
    r: &Rastro,
    eu: Vec2,
    destino_final: Option<Vec2>,
    altura: &dyn Fn(f32, f32) -> f32,
    cam: &Camera3D,
) {
    let Some(m) = restante(r, eu, destino_final) else { return };
    if m < 1.0 {
        return;
    }
    // Na viagem longa o numero e' do destino FINAL, entao mora nele; senao no
    // fim da rota.
    let Some(alvo) = destino_final.or_else(|| r.destino()) else { return };
    let topo = vec3(alvo.x, altura(alvo.x, alvo.y) + 1.1, alvo.y);
    let Some(c) = crate::render3d::world_to_screen(cam, topo) else { return };
    let texto = formata_distancia(m);
    let largura = texto.chars().count() as f32 * 8.0 + 18.0;
    crate::hud_estilo::painel(Rect::new(c.x - largura * 0.5, c.y - 24.0, largura, 22.0));
    crate::hud_estilo::texto_centro(c.x, c.y - 8.0, &texto, 15, crate::hud_estilo::OURO);
}

/// O ponto da amostra na distancia `s` (interpola entre vizinhas).
fn ponto_em(amostras: &[(Vec3, f32)], s: f32) -> Vec3 {
    let i = amostras.partition_point(|(_, d)| *d < s);
    if i == 0 {
        return amostras[0].0;
    }
    if i >= amostras.len() {
        return amostras[amostras.len() - 1].0;
    }
    let ((a, da), (b, db)) = (amostras[i - 1], amostras[i]);
    let t = if db - da > 1e-6 { (s - da) / (db - da) } else { 0.0 };
    a.lerp(b, t)
}

/// Os tracos como quads virados pra cima. `total` e' o comprimento do caminho
/// INTEIRO (a fase conta do fim), `t` o relogio.
/// Onde, ao longo do caminho (em `0..fim`), caem os tracos. A fase conta do
/// FIM do caminho (`total`): consumir o comeco nao mexe nos tracos no chao.
/// Com o tempo, cada traco anda pra frente (s maior).
pub fn intervalos(total: f32, t: f32, fim: f32) -> Vec<(f32, f32)> {
    let periodo = TRACO + VAO;
    let fase = (total + t * VELOCIDADE).rem_euclid(periodo);
    let k0 = ((0.0 - fase) / periodo).floor() as i32;
    let k1 = ((fim - fase + TRACO) / periodo).ceil() as i32;
    (k0..=k1)
        .filter_map(|k| {
            let fim_traco = fase + k as f32 * periodo;
            let (s0, s1) = ((fim_traco - TRACO).max(0.0), fim_traco.min(fim));
            (s1 - s0 >= 0.02).then_some((s0, s1))
        })
        .collect()
}

pub fn tracos(amostras: &[(Vec3, f32)], total: f32, t: f32, largura: f32) -> Vec<[Vec3; 4]> {
    let mut quads = Vec::new();
    let Some(&(_, fim)) = amostras.last() else { return quads };
    if amostras.len() < 2 {
        return quads;
    }
    for (s0, s1) in intervalos(total, t, fim) {
        // O traco dobra junto com o relevo: passa pelas amostras de dentro.
        let mut pts = vec![ponto_em(amostras, s0)];
        pts.extend(amostras.iter().filter(|(_, d)| *d > s0 && *d < s1).map(|(p, _)| *p));
        pts.push(ponto_em(amostras, s1));
        for par in pts.windows(2) {
            if let Some(q) = faixa(par[0], par[1], largura) {
                quads.push(q);
            }
        }
    }
    quads
}

/// Faixa plana de `a` a `b`, virada pra cima.
fn faixa(a: Vec3, b: Vec3, largura: f32) -> Option<[Vec3; 4]> {
    let dir = vec2(b.x - a.x, b.z - a.z);
    if dir.length() < 1e-4 {
        return None;
    }
    let lado = dir.normalize().perp() * (largura * 0.5);
    let l = vec3(lado.x, 0.0, lado.y);
    Some(para_cima([a - l, b - l, b + l, a + l]))
}

/// Ordem dos cantos pela conta, igual `construcoes::emite`: o material solido
/// descarta a face de costas, e o tracejado e' visto de cima.
fn para_cima(c: [Vec3; 4]) -> [Vec3; 4] {
    if (c[1] - c[0]).cross(c[2] - c[0]).y >= 0.0 {
        c
    } else {
        [c[0], c[3], c[2], c[1]]
    }
}

/// Anel achatado no chao, em `centro`.
fn anel(centro: Vec3, raio: f32, espessura: f32, lados: usize) -> Vec<[Vec3; 4]> {
    (0..lados)
        .map(|i| {
            let a0 = i as f32 / lados as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / lados as f32 * std::f32::consts::TAU;
            let p = |a: f32, r: f32| centro + vec3(a.cos() * r, 0.0, a.sin() * r);
            let (ri, re) = (raio - espessura * 0.5, raio + espessura * 0.5);
            para_cima([p(a0, ri), p(a1, ri), p(a1, re), p(a0, re)])
        })
        .collect()
}

/// Um "X" no chao.
fn xis(centro: Vec3, tamanho: f32, largura: f32) -> Vec<[Vec3; 4]> {
    let d = tamanho * 0.5;
    [(vec3(-d, 0.0, -d), vec3(d, 0.0, d)), (vec3(-d, 0.0, d), vec3(d, 0.0, -d))]
        .into_iter()
        .filter_map(|(a, b)| faixa(centro + a, centro + b, largura))
        .collect()
}

/// Quads na cor dada, ja' fatiados no teto de indice.
fn malhas(quads: &[[Vec3; 4]], cor: [u8; 4], saida: &mut Vec<(Vec<Vertex>, Vec<u16>)>) {
    for bloco in quads.chunks(MAX_QUADS) {
        let mut verts = Vec::with_capacity(bloco.len() * 4);
        let mut idx = Vec::with_capacity(bloco.len() * 6);
        for q in bloco {
            let i = verts.len() as u16;
            for p in q {
                verts.push(Vertex { position: *p, uv: vec2(0.0, 0.0), color: cor, normal: Vec4::ZERO });
            }
            idx.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
        }
        saida.push((verts, idx));
    }
}

/// Toda a geometria do quadro: rota tracejada, anel pulsando no destino e, se
/// a viagem vai alem da rota, um tracejado fraco e reto ate' o destino final.
pub fn geometria(
    r: &Rastro,
    eu: Vec2,
    destino_final: Option<Vec2>,
    altura: &dyn Fn(f32, f32) -> f32,
    t: f32,
) -> Vec<(Vec<Vertex>, Vec<u16>)> {
    let mut sombra = Vec::new();
    let mut claro = Vec::new();
    let mut fraco = Vec::new();
    let chao = |p: Vec2, extra: f32| vec3(p.x, altura(p.x, p.y) + ELEVACAO + extra, p.y);

    let caminho = r.caminho(eu);
    let fim_da_rota = caminho.last().copied();
    if caminho.len() >= 2 {
        let total = comprimento(&caminho);
        let amostras = amostrar(&caminho, altura, ALCANCE);
        sombra.extend(tracos(&amostras, total, t, LARGURA * 1.9));
        let acima: Vec<(Vec3, f32)> = amostras.iter().map(|(p, d)| (*p + vec3(0.0, 0.015, 0.0), *d)).collect();
        claro.extend(tracos(&acima, total, t, LARGURA));
    }
    if let Some(d) = r.destino() {
        let pulso = 0.55 + 0.12 * (t * 4.0).sin();
        let c = chao(d, 0.0);
        sombra.extend(anel(c, pulso, 0.16, 24));
        claro.extend(anel(c + vec3(0.0, 0.015, 0.0), pulso, 0.08, 24));
        claro.extend(xis(c + vec3(0.0, 0.015, 0.0), 0.45, 0.07));
    }
    if let Some(f) = destino_final {
        let de = fim_da_rota.unwrap_or(eu);
        if de.distance(f) > 3.0 {
            let reta = [de, f];
            let amostras = amostrar(&reta, altura, ALCANCE);
            fraco.extend(tracos(&amostras, comprimento(&reta), t, LARGURA * 0.75));
            fraco.extend(anel(chao(f, 0.0), 0.4, 0.07, 20));
        }
    }
    let mut saida = Vec::new();
    malhas(&sombra, SOMBRA, &mut saida);
    malhas(&fraco, FRACO, &mut saida);
    malhas(&claro, CLARO, &mut saida);
    saida
}

/// Desenha no passe do mundo (material solido ja' ligado).
pub fn desenha(r: &Rastro, eu: Vec2, destino_final: Option<Vec2>, altura: &dyn Fn(f32, f32) -> f32, t: f32) {
    if !r.ativo() && destino_final.is_none() {
        return;
    }
    for (vertices, indices) in geometria(r, eu, destino_final, altura, t) {
        draw_mesh(&Mesh { vertices, indices, texture: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_o_que_falta_pelo_caminho() {
        let mut r = Rastro::default();
        assert_eq!(restante(&r, Vec2::ZERO, None), None, "parado: sem contador");
        r.define(vec![vec2(10.0, 0.0), vec2(10.0, 10.0)], vec2(10.0, 10.0));
        let m = restante(&r, Vec2::ZERO, None).unwrap();
        assert!((m - 20.0).abs() < 1e-3, "pelo caminho (20), nao em reta: {m}");
        let m = restante(&r, Vec2::ZERO, Some(vec2(10.0, 40.0))).unwrap();
        assert!((m - 50.0).abs() < 1e-3, "viagem alem da rota soma a reta final: {m}");
        assert_eq!(formata_distancia(0.2), "1 m");
        assert_eq!(formata_distancia(12.3), "13 m");
        assert_eq!(formata_distancia(1540.0), "1,5 km");
    }

    #[test]
    fn descarta_pontos_alcancados_e_passados() {
        let mut r = Rastro::default();
        r.define(vec![vec2(5.0, 0.0), vec2(10.0, 0.0), vec2(10.0, 10.0)], vec2(10.0, 10.0));
        r.acompanha(vec2(0.0, 0.0));
        assert_eq!(r.caminho(vec2(0.0, 0.0)).len(), 4, "nada alcancado ainda");
        r.acompanha(vec2(4.6, 0.0));
        assert_eq!(r.pontos.len(), 2, "dentro do CHEGOU descarta");
        // Cortou a quina sem chegar a 0,6 do (10,0): passou dele.
        r.acompanha(vec2(9.5, 2.0));
        assert_eq!(r.pontos, vec![vec2(10.0, 10.0)]);
        r.acompanha(vec2(10.0, 9.7));
        assert!(!r.ativo() && r.destino().is_none(), "chegou: some");
        r.define(Vec::new(), vec2(0.0, 0.0));
        assert!(!r.ativo(), "rota vazia do servidor limpa");
    }

    #[test]
    fn a_amostra_segue_o_relevo() {
        let relevo = |x: f32, _z: f32| if x < 5.0 { 1.0 } else { 3.5 };
        let a = amostrar(&[vec2(0.0, 0.0), vec2(10.0, 0.0)], &relevo, ALCANCE);
        assert!(a.len() >= 20, "amostra a cada meio passo");
        for (p, _) in &a {
            let esperado = relevo(p.x, p.z) + ELEVACAO;
            assert!((p.y - esperado).abs() < 1e-4, "fora do chao em {p:?}");
        }
        // Alcance corta.
        let longe = amostrar(&[vec2(0.0, 0.0), vec2(500.0, 0.0)], &relevo, 30.0);
        assert!(longe.last().unwrap().1 <= 30.0 + PASSO);
    }

    #[test]
    fn os_tracos_andam_pra_frente_e_ficam_parados_no_chao() {
        // O traco que comeca depois de s = 5, no instante t.
        let comeco = |total: f32, t: f32, desloc: f32| {
            intervalos(total, t, 20.0)
                .iter()
                .map(|(s0, _)| s0 + desloc)
                .find(|s| *s > 5.0)
                .unwrap()
        };
        let (a, b) = (comeco(20.0, 0.0, 0.0), comeco(20.0, 0.1, 0.0));
        let andou = (b - a).rem_euclid(TRACO + VAO);
        assert!((andou - 0.1 * VELOCIDADE).abs() < 1e-3, "traco nao andou pra frente: {a} -> {b}");
        // O jogador andou 3 u pela rota: o caminho encurtou pelo COMECO, e os
        // tracos (em coordenada de mundo = s + 3) continuam no mesmo lugar.
        let (x1, x2) = (comeco(20.0, 0.0, 0.0), comeco(17.0, 0.0, 3.0));
        assert!((x1 - x2).abs() < 1e-3, "traco escorregou com o jogador: {x1} vs {x2}");
        // E a geometria usa esses intervalos: ha' traco e ha' vao.
        let plano = |_: f32, _: f32| 0.0;
        let c = [vec2(0.0, 0.0), vec2(20.0, 0.0)];
        let q = tracos(&amostrar(&c, &plano, ALCANCE), comprimento(&c), 0.0, LARGURA);
        let coberto: f32 = q.iter().map(|q| (q[0].x - q[1].x).abs().max((q[0].x - q[2].x).abs())).sum();
        assert!(coberto > 20.0 * 0.5 && coberto < 20.0 * 0.75, "cobertura {coberto}");
    }

    #[test]
    fn geometria_virada_pra_cima_e_dentro_do_teto_de_indice() {
        let mut r = Rastro::default();
        // Zigue-zague longo: muito traco.
        let pontos: Vec<Vec2> = (1..400).map(|i| vec2(i as f32 * 0.6, if i % 2 == 0 { 0.0 } else { 0.5 })).collect();
        let d = *pontos.last().unwrap();
        r.define(pontos, d);
        let relevo = |x: f32, _z: f32| (x * 0.3).sin();
        let g = geometria(&r, vec2(0.0, 0.0), Some(vec2(300.0, 40.0)), &relevo, 1.3);
        assert!(!g.is_empty());
        for (v, i) in &g {
            assert!(i.len() <= MAX_QUADS * 6 && v.len() <= MAX_QUADS * 4, "{} indices", i.len());
            for q in i.chunks(6) {
                let (a, b, c) = (v[q[0] as usize].position, v[q[1] as usize].position, v[q[2] as usize].position);
                assert!((b - a).cross(c - a).y >= -1e-6, "face de costas pra camera");
            }
        }
    }
}
