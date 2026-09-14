//! Onde fica cada coisa do HUD, no molde do MIR4 (docs/HUD.md, 2.1–2.4).
//!
//! UMA funcao responde: desenho, clique (`contem`) e o teste de sobreposicao
//! chamam `zonas`. Antes cada modulo calculava o proprio retangulo, e dois
//! deles se cobriam sem ninguem saber (o botao "Bolsa [I]" sobre o rotulo do
//! AUTO COMBATE, tres faixas de estado na mesma altura).
//!
//! Os numeros sao px a 1920×1080 e multiplicam pela escala.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

pub const BASE_W: f32 = 1920.0;
pub const BASE_H: f32 = 1080.0;

/// Escala da interface: a menor das duas razoes contra 1920×1080, presa entre
/// 0,70 (janela lado a lado) e 1,30 (monitor grande).
pub fn escala(sw: f32, sh: f32) -> f32 {
    (sw / BASE_W).min(sh / BASE_H).clamp(0.70, 1.30)
}

#[derive(Debug, Clone, Copy)]
pub struct Zonas {
    pub s: f32,
    /// Nivel, nome, HP, MP, vigor e Poder. Canto superior esquerdo.
    pub ficha: Rect,
    /// Linha de buffs logo abaixo da ficha.
    pub buffs: Rect,
    /// Area MAXIMA do rastreador de missoes (esquerda, abaixo dos buffs).
    pub rastreador: Rect,
    pub missoes_no_rastreador: usize,
    /// Alvo selecionado: no alto, centrado no vao entre a ficha e a direita.
    pub alvo: Rect,
    /// Bolsa, Missoes, Diarias, Grupo, Avisos — a' esquerda do MENU.
    pub icones: [Rect; 5],
    pub menu: Rect,
    /// Nome da zona e canal (clique abre o Mapa).
    pub area: Rect,
    pub minimapa: Rect,
    /// ⤢ dentro da moldura do minimapa: abre o Mapa.
    pub mapa_icone: Rect,
    pub chat: Rect,
    /// A faixa de estado UNICA (INDO, AUTO MISSAO, AUTO COMBATE…).
    pub faixa: Rect,
    /// A barrinha "Coletando · tipo · N s", logo abaixo da faixa.
    pub coleta: Rect,
    /// O botao grande de ataque (F).
    pub atacar: Rect,
    /// Skills 1, 2, 3 e o 4 reservado, em arco em volta do ATACAR.
    pub skills: [Rect; 4],
    pub auto_combate: Rect,
    pub auto_coleta: Rect,
    /// Pocao de vida (C).
    pub pocao: Rect,
    /// Slots rapidos 8, 9, 0.
    pub rapidos: [Rect; 3],
    pub exp: Rect,
}

/// As zonas da tela atual.
pub fn atual() -> Zonas {
    zonas(screen_width(), screen_height())
}

pub fn zonas(sw: f32, sh: f32) -> Zonas {
    let s = escala(sw, sh);
    let m = 20.0 * s;

    // ── esquerda: ficha, buffs, rastreador ──
    let ficha = Rect::new(m, m, 440.0 * s, 160.0 * s);
    let buffs = Rect::new(m, ficha.y + ficha.h + 6.0 * s, 440.0 * s, 40.0 * s);
    // 16:10 ganha uma missao; tela baixa perde.
    let n = if sh >= 1150.0 { 5 } else if sh < 700.0 { 2 } else { 4 };
    let rastreador = Rect::new(m, buffs.y + buffs.h + 8.0 * s, 440.0 * s, (48.0 + 56.0 * n as f32 + 28.0) * s);

    // ── topo direito: icones, MENU, area, minimapa ──
    let menu = Rect::new(sw - m - 64.0 * s, m, 64.0 * s, 48.0 * s);
    let (icone, vao) = (48.0 * s, 8.0 * s);
    let icones = [0usize, 1, 2, 3, 4].map(|i| {
        Rect::new(menu.x - 16.0 * s - (5 - i) as f32 * (icone + vao) + vao, m, icone, icone)
    });
    let area = Rect::new(sw - m - 320.0 * s, menu.y + menu.h + 12.0 * s, 320.0 * s, 56.0 * s);
    let minimapa = Rect::new(area.x, area.y + area.h + 4.0 * s, 320.0 * s, 320.0 * s);
    let mapa_icone = Rect::new(minimapa.x + minimapa.w - 32.0 * s, minimapa.y + 6.0 * s, 26.0 * s, 26.0 * s);

    // ── alvo: centrado no vao que sobra no alto ──
    let esq = ficha.x + ficha.w + 16.0 * s;
    let dir = icones[0].x.min(area.x) - 16.0 * s;
    let w = (460.0 * s).min(dir - esq).max(200.0 * s);
    let alvo = Rect::new((esq + dir) * 0.5 - w * 0.5, m, w, 80.0 * s);

    // ── baixo: EXP, chat, cluster de combate ──
    let exp = Rect::new(0.0, sh - 6.0, sw, 6.0);
    let base = sh - 10.0 * s;
    let chat = Rect::new(m, sh - 16.0 * s - 170.0 * s, 480.0 * s, 170.0 * s);
    let ca = vec2(sw - 110.0 * s, base - 110.0 * s);
    let ra = 60.0 * s;
    let atacar = Rect::new(ca.x - ra, ca.y - ra, ra * 2.0, ra * 2.0);
    // O 1 e' o mais perto do ATACAR (MIR4 numera a partir do ataque): 1 a'
    // esquerda, 2 na diagonal, 3 acima. O 4, reservado, fica a' esquerda do 2.
    // Em caixa e nao so' em circulo: e' a caixa que o clique e o teste medem.
    let rs = 42.0 * s;
    let skills = [vec2(-150.0, 0.0), vec2(-106.0, -106.0), vec2(0.0, -150.0), vec2(-256.0, -106.0)].map(|d| {
        let c = ca + d * s;
        Rect::new(c.x - rs, c.y - rs, rs * 2.0, rs * 2.0)
    });
    // Linha de baixo, a' esquerda do arco: [COLETA][COMBATE][C][8][9][0].
    let direita = sw - 312.0 * s;
    let yb = base - 14.0 * s;
    let rapido = 56.0 * s;
    let x0 = direita - 3.0 * rapido - 16.0 * s;
    let rapidos = [0usize, 1, 2].map(|i| Rect::new(x0 + i as f32 * (rapido + 8.0 * s), yb - rapido, rapido, rapido));
    let pocao = Rect::new(x0 - 10.0 * s - 64.0 * s, yb - 64.0 * s, 64.0 * s, 64.0 * s);
    let auto_combate = Rect::new(pocao.x - 14.0 * s - 88.0 * s, yb - 88.0 * s, 88.0 * s, 88.0 * s);
    let auto_coleta = Rect::new(auto_combate.x - 10.0 * s - 88.0 * s, yb - 88.0 * s, 88.0 * s, 88.0 * s);
    let fw = 520.0 * s;
    let faixa = Rect::new(sw * 0.5 - fw * 0.5, sh - 330.0 * s, fw, 34.0 * s);
    let cw = 380.0 * s;
    let coleta = Rect::new(sw * 0.5 - cw * 0.5, faixa.y + faixa.h + 6.0 * s, cw, 30.0 * s);

    Zonas {
        s,
        ficha,
        buffs,
        rastreador,
        missoes_no_rastreador: n,
        alvo,
        icones,
        menu,
        area,
        minimapa,
        mapa_icone,
        chat,
        faixa,
        coleta,
        atacar,
        skills,
        auto_combate,
        auto_coleta,
        pocao,
        rapidos,
        exp,
    }
}

impl Zonas {
    /// Todo retangulo de nivel de cima (sem os de dentro de outro, como o ⤢ do
    /// minimapa), com nome — pro teste e pro `contem`.
    pub fn todos(&self) -> Vec<(&'static str, Rect)> {
        let mut v = vec![
            ("ficha", self.ficha),
            ("buffs", self.buffs),
            ("rastreador", self.rastreador),
            ("alvo", self.alvo),
            ("menu", self.menu),
            ("area", self.area),
            ("minimapa", self.minimapa),
            ("chat", self.chat),
            ("faixa", self.faixa),
            ("coleta", self.coleta),
            ("atacar", self.atacar),
            ("auto_combate", self.auto_combate),
            ("auto_coleta", self.auto_coleta),
            ("pocao", self.pocao),
            ("exp", self.exp),
        ];
        for (i, r) in self.icones.iter().enumerate() {
            v.push((["icone_bolsa", "icone_missoes", "icone_diarias", "icone_grupo", "icone_avisos"][i], *r));
        }
        for (i, r) in self.skills.iter().enumerate() {
            v.push((["skill1", "skill2", "skill3", "skill4"][i], *r));
        }
        for (i, r) in self.rapidos.iter().enumerate() {
            v.push((["rapido8", "rapido9", "rapido0"][i], *r));
        }
        v
    }

    /// Botoes e paineis que pegam o clique (o mundo nao recebe). A faixa, o
    /// alvo e a EXP so' mostram; o rastreador e o chat entram pelo modulo
    /// deles (tamanho varia).
    pub fn contem(&self, p: Vec2) -> bool {
        [self.ficha, self.menu, self.area, self.minimapa, self.atacar, self.auto_combate, self.auto_coleta, self.pocao]
            .iter()
            .chain(self.icones.iter())
            .chain(self.skills.iter())
            .chain(self.rapidos.iter())
            .any(|r| r.contains(p))
            || self.alvo_fechar().contains(p)
    }

    /// O X pequeno do alvo (limpa a selecao).
    pub fn alvo_fechar(&self) -> Rect {
        let t = 22.0 * self.s.max(0.8);
        Rect::new(self.alvo.x + self.alvo.w - t - 4.0, self.alvo.y + 4.0, t, t)
    }
}

/// Alt segurado: mostra a tecla de cada botao de acao (MIR4).
pub fn alt() -> bool {
    is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt)
}

/// O chip de tecla no canto de um botao, so' com Alt.
pub fn chip(r: Rect, tecla: &str) {
    if !alt() {
        return;
    }
    estilo::chip_tecla(vec2(r.x, r.y), tecla);
}

/// A faixa de estado unica, no centro. Quem decide o texto e' o `main`.
pub fn desenha_faixa(z: &Zonas, texto: &str, cor: Color) {
    let w = (estilo::medir_forte(texto, 14) + 52.0).clamp(z.faixa.w * 0.5, z.faixa.w.max(240.0));
    let r = Rect::new(z.faixa.center().x - w * 0.5, z.faixa.y, w, z.faixa.h);
    let raio = r.h * 0.5;
    // Pilula: vidro escuro, borda na cor do estado e um ponto pulsando.
    estilo::sombra(r, raio, 1.0);
    estilo::ret_gradiente(r, raio, estilo::FUNDO_ALTO, estilo::FUNDO);
    estilo::borda_arredondada(r, raio, 1.0, estilo::alfa(cor, 0.45));
    let pulso = 0.6 + 0.4 * (get_time() as f32 * 3.0).sin();
    draw_circle(r.x + raio + 2.0, r.center().y, 4.0, estilo::alfa(cor, pulso));
    estilo::texto_centro_forte(r.center().x + 6.0, r.y + r.h * 0.5 + 5.0, texto, 14, cor);
}

/// Mundo escurecido atras de painel grande (Menu, Bolsa, Mapa, Craft…).
pub fn escurece(alfa: f32) {
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, alfa));
}

#[cfg(test)]
mod tests {
    use super::*;

    const TELAS: [(f32, f32); 8] = [
        (1920.0, 1080.0),
        (1920.0, 1200.0),
        (1280.0, 720.0),
        (1366.0, 768.0),
        (1600.0, 900.0),
        (1024.0, 768.0),
        (960.0, 1080.0),
        // A janela padrao do cliente (lado a lado com o terminal).
        (940.0, 980.0),
    ];

    fn cruzam(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn nada_do_hud_se_sobrepoe() {
        for (sw, sh) in TELAS {
            let z = zonas(sw, sh);
            let todos = z.todos();
            for i in 0..todos.len() {
                for j in i + 1..todos.len() {
                    let ((na, a), (nb, b)) = (todos[i], todos[j]);
                    assert!(!cruzam(a, b), "{sw}×{sh}: {na} {a:?} cobre {nb} {b:?}");
                }
            }
        }
    }

    #[test]
    fn tudo_cabe_na_tela() {
        for (sw, sh) in TELAS {
            for (nome, r) in zonas(sw, sh).todos() {
                assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= sw + 0.01 && r.y + r.h <= sh + 0.01,
                    "{sw}×{sh}: {nome} {r:?} sai da tela");
            }
        }
    }

    #[test]
    fn escala_e_ancoras() {
        assert_eq!(escala(1920.0, 1080.0), 1.0);
        assert_eq!(escala(1920.0, 1200.0), 1.0, "16:10 limitado pela largura");
        assert_eq!(escala(940.0, 980.0), 0.70);
        let z = zonas(1920.0, 1080.0);
        assert_eq!(z.ficha.x, 20.0);
        assert!(z.menu.x + z.menu.w <= 1900.01, "MENU no canto superior direito");
        assert!(z.atacar.center().x > 1700.0 && z.atacar.center().y > 900.0, "ATACAR no canto inferior direito");
        assert!(z.skills[0].center().x < z.atacar.center().x, "o 1 fica a' esquerda do ATACAR");
        assert!(z.mapa_icone.x >= z.minimapa.x && z.mapa_icone.y >= z.minimapa.y, "⤢ dentro do minimapa");
        assert_eq!(zonas(1920.0, 1200.0).missoes_no_rastreador, 5);
        assert_eq!(z.missoes_no_rastreador, 4);
    }

    /// O icone das Diarias mora no topo direito, logo depois de Missoes e antes
    /// do MENU, em toda tela.
    #[test]
    fn icone_das_diarias_ao_lado_de_missoes() {
        for (sw, sh) in TELAS {
            let z = zonas(sw, sh);
            let (missoes, diarias) = (z.icones[1], z.icones[2]);
            assert!(diarias.x > missoes.x + missoes.w, "{sw}×{sh}: diarias a' direita de missoes");
            assert!(diarias.x + diarias.w < z.menu.x, "{sw}×{sh}: diarias antes do MENU");
            assert_eq!(diarias.y, missoes.y);
        }
    }
}
