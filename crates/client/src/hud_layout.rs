//! Onde fica cada coisa do HUD, no molde do MIR4 (docs/HUD.md, 2.1–2.4).
//!
//! UMA funcao responde: desenho, clique (`contem`) e o teste de sobreposicao
//! chamam `zonas`. Antes cada modulo calculava o proprio retangulo, e dois
//! deles se cobriam sem ninguem saber (o botao "Bolsa [I]" sobre o rotulo do
//! AUTO COMBATE, tres faixas de estado na mesma altura).
//!
//! Os numeros sao px a 1920×1080 e multiplicam pela escala.
use macroquad::prelude::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::hud_estilo as estilo;

pub const BASE_W: f32 = 1920.0;
pub const BASE_H: f32 = 1080.0;

/// Escala da interface: a menor das duas razoes contra 1920×1080, presa entre
/// 0,70 (janela lado a lado) e 1,30 (monitor grande).
pub fn escala(sw: f32, sh: f32) -> f32 {
    (sw / BASE_W).min(sh / BASE_H).clamp(0.70, 1.30)
}

// ───────────── escala escolhida e area segura (celular) ─────────────

/// Faixa da escala da interface (Menu → Sistema → Interface): multiplica o
/// HUD e todo texto que passa pelo `hud_estilo`.
pub const ESCALA_UI_MIN: f32 = 0.8;
pub const ESCALA_UI_MAX: f32 = 1.6;

/// Celular: tela pequena e densa, o texto de 14 px some. PC fica em 100%.
pub fn escala_ui_padrao() -> f32 {
    if crate::nativo::TECLADO_NA_TELA {
        1.3
    } else {
        1.0
    }
}

/// Bits do f32; 0 = nunca escolheu (vale o padrao da plataforma).
static ESCALA_UI: AtomicU32 = AtomicU32::new(0);
/// Area segura em px: topo, esquerda, baixo, direita.
static MARGENS: [AtomicU32; 4] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
/// Minimapa grande (0 = compacto). Entra na chave do memo do `atual`.
static MINIMAPA_GRANDE: AtomicU32 = AtomicU32::new(0);
static MINIMAPA_OCULTO: AtomicU32 = AtomicU32::new(0);

pub fn escala_ui() -> f32 {
    match ESCALA_UI.load(Ordering::Relaxed) {
        0 => escala_ui_padrao(),
        b => f32::from_bits(b),
    }
}

/// Em passos de 10%, dentro da faixa. Vale no quadro seguinte.
pub fn define_escala_ui(v: f32) {
    let v = if v.is_finite() {
        ((v * 10.0).round() / 10.0).clamp(ESCALA_UI_MIN, ESCALA_UI_MAX)
    } else {
        escala_ui_padrao()
    };
    ESCALA_UI.store(v.to_bits(), Ordering::Relaxed);
}

pub fn margens() -> [f32; 4] {
    MARGENS
        .each_ref()
        .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
}

pub fn define_margens(m: [f32; 4]) {
    for (a, v) in MARGENS.iter().zip(m) {
        a.store(
            if v.is_finite() { v.max(0.0) } else { 0.0 }.to_bits(),
            Ordering::Relaxed,
        );
    }
}

/// O minimapa esta' grande? Quem escolhe e' o jogador, no botao da moldura, e
/// a escolha vai pras preferencias do personagem.
pub fn minimapa_expandido() -> bool {
    MINIMAPA_GRANDE.load(Ordering::Relaxed) != 0
}

pub fn define_minimapa_expandido(v: bool) {
    MINIMAPA_GRANDE.store(v as u32, Ordering::Relaxed);
}

/// O minimapa esta' fechado? Fica so' o botao redondo no canto dele (o do ⤢),
/// que reabre. Vai pras preferencias como o tamanho.
pub fn minimapa_oculto() -> bool {
    MINIMAPA_OCULTO.load(Ordering::Relaxed) != 0
}

pub fn define_minimapa_oculto(v: bool) {
    MINIMAPA_OCULTO.store(v as u32, Ordering::Relaxed);
}

/// Uma vez por quadro: rele' a area segura a cada 30 (girar o aparelho muda o
/// lado do notch).
pub fn acompanhar() {
    thread_local!(static QUADRO: Cell<u32> = const { Cell::new(0) });
    let n = QUADRO.with(|q| q.replace(q.get().wrapping_add(1)));
    if n % 30 == 0 {
        define_margens(crate::nativo::area_segura());
    }
}

/// A tela menos a area segura: onde painel ancorado em canto deve ficar.
pub fn tela_segura() -> Rect {
    let [t, e, b, d] = margens();
    let (sw, sh) = crate::render3d::tela();
    Rect::new(e, t, (sw - e - d).max(64.0), (sh - t - b).max(64.0))
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
    /// Bolsa, Missoes, Diarias, Grupo, Avisos, Presenca — a' esquerda do MENU.
    pub icones: [Rect; 6],
    pub menu: Rect,
    /// Nome da zona e canal (clique abre o Mapa).
    pub area: Rect,
    pub minimapa: Rect,
    /// ⤢ dentro da moldura do minimapa: abre o Mapa.
    pub mapa_icone: Rect,
    /// Bateria do modo economia: canto inferior esquerdo, sempre visivel.
    pub economia: Rect,
    /// Montar/desmontar: ao lado da bateria, sempre visivel.
    pub montaria: Rect,
    /// Botao de PULO: existe pro celular, que nao tem a tecla de espaco.
    pub pulo: Rect,
    /// AVISOS: as tres ultimas mensagens, em texto solto logo abaixo do
    /// rastreador. Nao pega toque — a faixa inteira ali e' do joystick.
    pub avisos: Rect,
    /// Onde um dedo pode COMECAR o joystick virtual (metade esquerda de baixo,
    /// ate' antes da faixa central e dos botoes AUTO). Nao e' botao: e' area.
    pub joystick: Rect,
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

/// Onde um dedo pode COMECAR o joystick: o quadrado inferior esquerdo da area
/// segura inteiro (metade da largura, metade de baixo da altura). Os botoes
/// que ficam ali dentro (bateria, montaria) continuam botoes: quem chama tira
/// `Zonas::contem`.
pub fn quadrante_do_joystick() -> Rect {
    let t = tela_segura();
    Rect::new(t.x, t.y + t.h * 0.5, t.w * 0.5, t.h * 0.5)
}

/// As zonas da tela atual.
/// Com a area segura e a escala escolhida. Chamado varias vezes por quadro:
/// guarda a ultima resposta (o ajuste abaixo testa sobreposicao).
pub fn atual() -> Zonas {
    thread_local!(static MEMO: Cell<Option<([u32; 8], Zonas)>> = const { Cell::new(None) });
    let (sw, sh) = crate::render3d::tela();
    let (mg, ui) = (margens(), escala_ui());
    let grande = minimapa_expandido();
    let mut chave = [0u32; 8];
    chave[..7].copy_from_slice(&[sw, sh, mg[0], mg[1], mg[2], mg[3], ui].map(f32::to_bits));
    chave[7] = grande as u32;
    MEMO.with(|c| match c.get() {
        Some((k, z)) if k == chave => z,
        _ => {
            let z = zonas_com(sw, sh, mg, ui, grande);
            c.set(Some((chave, z)));
            z
        }
    })
}

/// Tela inteira, 100%: a dos testes de PC.
#[cfg(test)]
pub fn zonas(sw: f32, sh: f32) -> Zonas {
    zonas_com(sw, sh, [0.0; 4], 1.0, false)
}

/// A mesma tela com o minimapa grande: o que vale pequeno tem que valer grande.
#[cfg(test)]
pub fn zonas_grandes(sw: f32, sh: f32) -> Zonas {
    zonas_com(sw, sh, [0.0; 4], 1.0, true)
}

/// `margens` = area segura (topo, esquerda, baixo, direita) em px; `ui` = a
/// escala escolhida. O HUD e' montado dentro da area segura. Se a escala pedida
/// nao couber (sobrepoe, sai da area ou o joystick fica sem polegar), desce aos
/// poucos ate' caber — nunca abaixo da escala da tela × min(ui, 1).
pub fn zonas_com(sw: f32, sh: f32, margens: [f32; 4], ui: f32, minimapa_grande: bool) -> Zonas {
    let [t, e, b, d] = margens;
    let (w, h) = ((sw - e - d).max(64.0), (sh - t - b).max(64.0));
    let base = escala(w, h);
    let piso = base * ui.min(1.0);
    let mut s = base * ui.max(0.1);
    loop {
        let z = monta(w, h, s, minimapa_grande);
        if s <= piso + 1e-4 || cabe(&z, w, h) {
            return z.desloca(vec2(e, t));
        }
        s = (s - 0.02).max(piso);
    }
}

fn cruzam(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// Tudo dentro de `w`×`h`, nada cobrindo nada e joystick com espaco pro polegar.
pub fn cabe(z: &Zonas, w: f32, h: f32) -> bool {
    let todos = z.todos();
    let dentro = todos.iter().all(|(_, r)| {
        r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= w + 0.01 && r.y + r.h <= h + 0.01
    });
    let livre =
        (0..todos.len()).all(|i| (i + 1..todos.len()).all(|j| !cruzam(todos[i].1, todos[j].1)));
    let polegar = 2.0 * crate::joystick::RAIO_BASE * z.s;
    dentro && livre && z.joystick.w >= polegar && z.joystick.h >= polegar
}

fn monta(sw: f32, sh: f32, s: f32, minimapa_grande: bool) -> Zonas {
    let m = 20.0 * s;

    // ── esquerda: ficha, buffs, rastreador ──
    let ficha = Rect::new(m, m, 440.0 * s, 160.0 * s);
    let buffs = Rect::new(m, ficha.y + ficha.h + 6.0 * s, 440.0 * s, 40.0 * s);
    // 16:10 ganha uma missao; tela baixa perde. Medido na altura "de 1080"
    // (px / escala): no celular a 130% sobra menos altura que os px sugerem.
    let hb = sh / s;
    let n = if hb >= 1150.0 {
        5
    } else if hb >= 960.0 {
        4
    } else if hb >= 860.0 {
        3
    } else {
        2
    };
    let rastreador = Rect::new(
        m,
        buffs.y + buffs.h + 8.0 * s,
        440.0 * s,
        (48.0 + 56.0 * n as f32 + 28.0) * s,
    );

    // ── topo direito: icones, MENU, area, minimapa ──
    let menu = Rect::new(sw - m - 64.0 * s, m, 64.0 * s, 48.0 * s);
    let (icone, vao) = (48.0 * s, 8.0 * s);
    let icones = [0usize, 1, 2, 3, 4, 5].map(|i| {
        Rect::new(
            menu.x - 16.0 * s - (6 - i) as f32 * (icone + vao) + vao,
            m,
            icone,
            icone,
        )
    });
    let area = Rect::new(
        sw - m - 320.0 * s,
        menu.y + menu.h + 12.0 * s,
        320.0 * s,
        56.0 * s,
    );
    let minimapa = Rect::new(area.x, area.y + area.h + 4.0 * s, 320.0 * s, 320.0 * s);
    let mapa_icone = Rect::new(
        minimapa.x + minimapa.w - 32.0 * s,
        minimapa.y + 6.0 * s,
        26.0 * s,
        26.0 * s,
    );

    // ── alvo: centrado no vao que sobra no alto ──
    let esq = ficha.x + ficha.w + 16.0 * s;
    let dir = icones[0].x.min(area.x) - 16.0 * s;
    let w = (460.0 * s).min(dir - esq).max(200.0 * s);
    let alvo = Rect::new((esq + dir) * 0.5 - w * 0.5, m, w, 80.0 * s);

    // ── baixo: EXP, chat, cluster de combate ──
    let exp = Rect::new(0.0, sh - 6.0, sw, 6.0);
    let base = sh - 10.0 * s;
    // Abaixo do rastreador: o canto inferior esquerdo ficou pro joystick.
    let avisos = Rect::new(
        m,
        rastreador.y + rastreador.h + 8.0 * s,
        440.0 * s,
        68.0 * s,
    );
    let ca = vec2(sw - 110.0 * s, base - 110.0 * s);
    let ra = 60.0 * s;
    let atacar = Rect::new(ca.x - ra, ca.y - ra, ra * 2.0, ra * 2.0);
    // O 1 e' o mais perto do ATACAR (MIR4 numera a partir do ataque): 1 a'
    // esquerda, 2 na diagonal, 3 acima. O 4, reservado, fica a' esquerda do 2.
    // Em caixa e nao so' em circulo: e' a caixa que o clique e o teste medem.
    let rs = 42.0 * s;
    let skills = [
        vec2(-150.0, 0.0),
        vec2(-106.0, -106.0),
        vec2(0.0, -150.0),
        vec2(-256.0, -106.0),
    ]
    .map(|d| {
        let c = ca + d * s;
        Rect::new(c.x - rs, c.y - rs, rs * 2.0, rs * 2.0)
    });
    // PULO: e' o QUARTO slot do arco, que estava reservado e so' desenhava um
    // disco cinza sem funcao. O celular nao tem tecla de espaco, e este e' o
    // lugar que o polegar ja' procura.
    let pulo = skills[3];
    // Linha de baixo, a' esquerda do arco: [COLETA][COMBATE][C][8][9][0].
    let direita = sw - 312.0 * s;
    let yb = base - 14.0 * s;
    let rapido = 56.0 * s;
    let x0 = direita - 3.0 * rapido - 16.0 * s;
    let rapidos = [0usize, 1, 2].map(|i| {
        Rect::new(
            x0 + i as f32 * (rapido + 8.0 * s),
            yb - rapido,
            rapido,
            rapido,
        )
    });
    let pocao = Rect::new(x0 - 10.0 * s - 64.0 * s, yb - 64.0 * s, 64.0 * s, 64.0 * s);
    let auto_combate = Rect::new(
        pocao.x - 14.0 * s - 88.0 * s,
        yb - 88.0 * s,
        88.0 * s,
        88.0 * s,
    );
    let auto_coleta = Rect::new(
        auto_combate.x - 10.0 * s - 88.0 * s,
        yb - 88.0 * s,
        88.0 * s,
        88.0 * s,
    );
    let fw = 520.0 * s;
    let faixa = Rect::new(sw * 0.5 - fw * 0.5, sh - 330.0 * s, fw, 34.0 * s);
    let cw = 380.0 * s;
    let coleta = Rect::new(
        sw * 0.5 - cw * 0.5,
        faixa.y + faixa.h + 6.0 * s,
        cw,
        30.0 * s,
    );
    // Joystick: TODA a faixa esquerda de baixo, do rastreador ate' a EXP e ate'
    // antes do que estiver mais a' esquerda entre a faixa, a barra de coleta e
    // o AUTO COLETA. Os avisos passam por cima sem pegar o toque — o dedo pode
    // comecar em qualquer ponto da faixa (pedido do dono).
    let jy = rastreador.y + rastreador.h + 8.0 * s;
    let jfim = faixa.x.min(coleta.x).min(auto_coleta.x) - 16.0 * s;
    // Bateria do modo economia: canto de baixo a' esquerda, sempre na tela. O
    // joystick termina acima dela.
    let lado_eco = 48.0 * s;
    let economia = Rect::new(m, exp.y - 8.0 * s - lado_eco, lado_eco, lado_eco);
    let montaria = Rect::new(
        economia.x + economia.w + 10.0 * s,
        economia.y,
        lado_eco,
        lado_eco,
    );
    let joystick = Rect::new(
        m,
        jy,
        (jfim - m).max(0.0),
        (economia.y - 8.0 * s - jy).max(0.0),
    );

    // Minimapa grande: cresce pra baixo, preso a' direita, e PARA antes do arco
    // de skills. O limite nao e' enfeite: sem ele o minimapa cruzaria o arco,
    // `cabe` diria que nao cabe e o laco do `zonas_com` encolheria o HUD
    // INTEIRO pra caber — o jogador pede um minimapa maior e recebe uma
    // interface menor.
    let (minimapa, mapa_icone) = if minimapa_grande {
        let teto = skills.iter().map(|r| r.y).fold(atacar.y, f32::min) - 12.0 * s;
        let lado = (520.0 * s).min(teto - minimapa.y).max(minimapa.w);
        let r = Rect::new(sw - m - lado, minimapa.y, lado, lado);
        (
            r,
            Rect::new(r.x + r.w - 32.0 * s, r.y + 6.0 * s, 26.0 * s, 26.0 * s),
        )
    } else {
        (minimapa, mapa_icone)
    };

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
        economia,
        montaria,
        pulo,
        avisos,
        joystick,
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
    /// Tudo arrastado de `o` (o canto da area segura).
    fn desloca(mut self, o: Vec2) -> Zonas {
        let d = move |r: Rect| Rect::new(r.x + o.x, r.y + o.y, r.w, r.h);
        for r in [
            &mut self.ficha,
            &mut self.buffs,
            &mut self.rastreador,
            &mut self.alvo,
            &mut self.menu,
            &mut self.area,
            &mut self.minimapa,
            &mut self.mapa_icone,
            &mut self.economia,
            &mut self.montaria,
            &mut self.pulo,
            &mut self.avisos,
            &mut self.joystick,
            &mut self.faixa,
            &mut self.coleta,
            &mut self.atacar,
            &mut self.auto_combate,
            &mut self.auto_coleta,
            &mut self.pocao,
            &mut self.exp,
        ] {
            *r = d(*r);
        }
        self.icones = self.icones.map(d);
        self.skills = self.skills.map(d);
        self.rapidos = self.rapidos.map(d);
        self
    }

    /// Todo retangulo de nivel de cima (sem os de dentro de outro, como o ⤢ do
    /// minimapa, e sem os AVISOS, que sao texto solto dentro da faixa do
    /// joystick), com nome — pro teste e pro `contem`.
    pub fn todos(&self) -> Vec<(&'static str, Rect)> {
        let mut v = vec![
            ("ficha", self.ficha),
            ("buffs", self.buffs),
            ("rastreador", self.rastreador),
            ("alvo", self.alvo),
            ("menu", self.menu),
            ("area", self.area),
            ("minimapa", self.minimapa),
            ("joystick", self.joystick),
            ("economia", self.economia),
            ("montaria", self.montaria),
            ("faixa", self.faixa),
            ("coleta", self.coleta),
            ("atacar", self.atacar),
            ("auto_combate", self.auto_combate),
            ("auto_coleta", self.auto_coleta),
            ("pocao", self.pocao),
            ("exp", self.exp),
        ];
        for (i, r) in self.icones.iter().enumerate() {
            v.push((
                [
                    "icone_bolsa",
                    "icone_missoes",
                    "icone_diarias",
                    "icone_grupo",
                    "icone_avisos",
                    "icone_presenca",
                ][i],
                *r,
            ));
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
        // Minimapa fechado: so' o botao de reabrir pega o toque, o resto do
        // canto volta a ser mundo.
        let minimapa = if minimapa_oculto() {
            self.minimapa_reabrir()
        } else {
            self.minimapa
        };
        [
            self.ficha,
            self.menu,
            self.area,
            minimapa,
            self.atacar,
            self.auto_combate,
            self.auto_coleta,
            self.pocao,
            self.economia,
            self.montaria,
            self.pulo,
        ]
        .iter()
        .chain(self.icones.iter())
        .chain(self.skills.iter())
        .chain(self.rapidos.iter())
        .any(|r| r.contains(p))
            || self.alvo_fechar().contains(p)
    }

    /// Minimapa fechado: o botao redondo que reabre, no canto de cima a'
    /// direita de onde ele fica. Grande o bastante pro dedo.
    pub fn minimapa_reabrir(&self) -> Rect {
        let lado = 46.0 * self.s;
        Rect::new(
            self.minimapa.x + self.minimapa.w - lado - 4.0 * self.s,
            self.minimapa.y + 4.0 * self.s,
            lado,
            lado,
        )
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
    draw_circle(
        r.x + raio + 2.0,
        r.center().y,
        4.0,
        estilo::alfa(cor, pulso),
    );
    estilo::texto_centro_forte(r.center().x + 6.0, r.y + r.h * 0.5 + 5.0, texto, 14, cor);
}

/// Mundo escurecido atras de painel grande (Menu, Bolsa, Mapa, Craft…).
pub fn escurece(alfa: f32) {
    let (sw, sh) = crate::render3d::tela();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, alfa));
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

    /// Area segura ja' com o respiro do `nativo` (px, paisagem): topo,
    /// esquerda, baixo, direita.
    const APARELHOS: [(&str, f32, f32, [f32; 4]); 5] = [
        ("iPhone 15 Pro", 2556.0, 1179.0, [48.0, 189.0, 75.0, 189.0]),
        (
            "iPhone 15 Pro Max",
            2796.0,
            1290.0,
            [48.0, 189.0, 75.0, 189.0],
        ),
        ("iPhone SE", 1334.0, 750.0, [32.0, 32.0, 32.0, 32.0]),
        ("iPhone 11", 1792.0, 828.0, [32.0, 96.0, 50.0, 96.0]),
        ("iPad Air", 2360.0, 1640.0, [48.0, 48.0, 90.0, 48.0]),
    ];

    #[test]
    fn toda_escala_cabe_em_todo_aparelho() {
        let telas = TELAS
            .iter()
            .map(|&(w, h)| ("PC", w, h, [0.0; 4]))
            .chain(APARELHOS);
        for (nome, sw, sh, mg) in telas {
            let seguro = Rect::new(mg[1], mg[0], sw - mg[1] - mg[3], sh - mg[0] - mg[2]);
            for ui in [ESCALA_UI_MIN, 1.0, 1.3, ESCALA_UI_MAX] {
                for grande in [false, true] {
                    let z = zonas_com(sw, sh, mg, ui, grande);
                    let local = z.desloca(vec2(-seguro.x, -seguro.y));
                    assert!(
                        cabe(&local, seguro.w, seguro.h),
                        "{nome} {sw}×{sh} a {ui} (grande {grande}): nao cabe na area segura (s {})",
                        z.s
                    );
                    assert!(
                        z.s + 1e-4 >= escala(seguro.w, seguro.h) * ui.min(1.0),
                        "{nome} a {ui} (grande {grande}): encolheu demais"
                    );
                    assert!(
                        !z.contem(z.joystick.center()),
                        "{nome} a {ui} (grande {grande}): meio do joystick cai num botao"
                    );
                }
            }
        }
    }

    #[test]
    fn celular_a_130_fica_maior_e_longe_do_notch() {
        let (_, sw, sh, mg) = APARELHOS[0];
        let (cem, cento_e_trinta) = (
            zonas_com(sw, sh, mg, 1.0, false),
            zonas_com(sw, sh, mg, 1.3, false),
        );
        assert!(
            cento_e_trinta.s > cem.s * 1.2,
            "130% cresceu so' {} → {}",
            cem.s,
            cento_e_trinta.s
        );
        for z in [cem, cento_e_trinta] {
            assert!(
                z.ficha.x >= mg[1] && z.ficha.y >= mg[0],
                "ficha no notch/canto: {:?}",
                z.ficha
            );
            assert!(
                z.menu.x + z.menu.w <= sw - mg[3],
                "MENU no notch: {:?}",
                z.menu
            );
            assert!(
                z.exp.y + z.exp.h <= sh - mg[2],
                "EXP na barra do home: {:?}",
                z.exp
            );
            assert!(z.atacar.x + z.atacar.w <= sw - mg[3], "ATACAR no notch");
        }
    }

    #[test]
    fn escala_escolhida_fica_na_faixa() {
        define_escala_ui(9.0);
        assert_eq!(escala_ui(), ESCALA_UI_MAX);
        define_escala_ui(1.26);
        assert!((escala_ui() - 1.3).abs() < 1e-6);
        define_escala_ui(f32::NAN);
        assert_eq!(escala_ui(), escala_ui_padrao());
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
                assert!(
                    r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= sw + 0.01 && r.y + r.h <= sh + 0.01,
                    "{sw}×{sh}: {nome} {r:?} sai da tela"
                );
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
        assert!(
            z.menu.x + z.menu.w <= 1900.01,
            "MENU no canto superior direito"
        );
        assert!(
            z.atacar.center().x > 1700.0 && z.atacar.center().y > 900.0,
            "ATACAR no canto inferior direito"
        );
        assert!(
            z.skills[0].center().x < z.atacar.center().x,
            "o 1 fica a' esquerda do ATACAR"
        );
        assert!(
            z.mapa_icone.x >= z.minimapa.x && z.mapa_icone.y >= z.minimapa.y,
            "⤢ dentro do minimapa"
        );
        assert_eq!(zonas(1920.0, 1200.0).missoes_no_rastreador, 5);
        assert_eq!(z.missoes_no_rastreador, 4);
    }

    /// Os avisos sao texto solto no alto da faixa do joystick, e a faixa
    /// inteira (do rastreador ate' a EXP) aceita o polegar.
    #[test]
    fn avisos_em_cima_e_joystick_na_faixa_inteira() {
        for (sw, sh) in TELAS {
            let z = zonas(sw, sh);
            assert!(
                z.avisos.y >= z.rastreador.y + z.rastreador.h,
                "{sw}×{sh}: avisos abaixo do rastreador"
            );
            assert!(
                z.joystick.y <= z.avisos.y + 0.01,
                "{sw}×{sh}: a faixa do joystick comeca no alto dos avisos"
            );
            assert!(
                z.joystick.contains(z.avisos.center()),
                "{sw}×{sh}: os avisos ficam DENTRO da faixa do joystick"
            );
            assert!(
                z.avisos.x >= 0.0
                    && z.avisos.y >= 0.0
                    && z.avisos.x + z.avisos.w <= sw
                    && z.avisos.y + z.avisos.h <= sh,
                "{sw}×{sh}: avisos fora da tela"
            );
            assert!(
                z.joystick.x + z.joystick.w <= sw * 0.5 + 0.01,
                "{sw}×{sh}: joystick na metade esquerda"
            );
            let min = 2.0 * crate::joystick::RAIO_BASE * z.s;
            assert!(
                z.joystick.w >= min && z.joystick.h >= min,
                "{sw}×{sh}: joystick pequeno demais {:?}",
                z.joystick
            );
            assert!(
                !z.contem(z.joystick.center()),
                "{sw}×{sh}: meio do joystick cai num botao"
            );
        }
    }

    /// Expandir o minimapa cresce o minimapa — e mais nada. Em especial nao
    /// pode encolher o resto do HUD (seria o laco do `zonas_com` "resolvendo"
    /// uma sobreposicao com o arco de skills).
    #[test]
    fn minimapa_grande_cresce_sem_encolher_o_hud() {
        for (sw, sh) in TELAS {
            let (pequeno, grande) = (zonas(sw, sh), zonas_grandes(sw, sh));
            assert_eq!(
                grande.s, pequeno.s,
                "{sw}×{sh}: expandir o minimapa encolheu o HUD inteiro"
            );
            assert!(
                grande.minimapa.w >= pequeno.minimapa.w,
                "{sw}×{sh}: o expandido nao cresceu"
            );
            assert!(
                grande.minimapa.w == grande.minimapa.h,
                "{sw}×{sh}: minimapa deixou de ser quadrado"
            );
            assert!(
                grande.mapa_icone.x >= grande.minimapa.x
                    && grande.mapa_icone.y >= grande.minimapa.y,
                "⤢ fora do minimapa"
            );
            let todos = grande.todos();
            for i in 0..todos.len() {
                for j in i + 1..todos.len() {
                    let ((na, a), (nb, b)) = (todos[i], todos[j]);
                    assert!(
                        !cruzam(a, b),
                        "{sw}×{sh} com minimapa grande: {na} {a:?} cobre {nb} {b:?}"
                    );
                }
            }
        }
    }

    /// O icone das Diarias mora no topo direito, logo depois de Missoes e antes
    /// do MENU, em toda tela.
    #[test]
    fn icone_das_diarias_ao_lado_de_missoes() {
        for (sw, sh) in TELAS {
            let z = zonas(sw, sh);
            let (missoes, diarias) = (z.icones[1], z.icones[2]);
            assert!(
                diarias.x > missoes.x + missoes.w,
                "{sw}×{sh}: diarias a' direita de missoes"
            );
            assert!(
                diarias.x + diarias.w < z.menu.x,
                "{sw}×{sh}: diarias antes do MENU"
            );
            assert_eq!(diarias.y, missoes.y);
        }
    }
}
