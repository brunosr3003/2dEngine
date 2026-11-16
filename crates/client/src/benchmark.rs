//! THE FIRST-LOGIN PERFORMANCE TEST (docs/BENCHMARK.md).
//!
//! Once per device, the first time the character screen shows, the client
//! renders three local scenes with no server — a big fight, the heaviest town
//! view and the open sea — at each graphics preset from heavy to light, and
//! measures the frame time (CPU and presented). The player then picks a goal
//! (Performance, Balanced, Quality) and a 30/60 fps target, and the highest
//! preset that holds that target is applied through `config_graficos`.
//!
//! The pure parts (percentiles, the pass rule, the choice, the saved file and
//! the runtime guard) sit at the top and are unit tested; the drawing of the
//! scenes is below; the screens are in `benchmark_ui`.
//!
//! The test runs one frame at a time from `Jogo::desenhar` (and not in a loop
//! of its own) so the connection keeps being pumped while it runs.

use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use macroquad::prelude::*;

use crate::config_graficos::{self as cg, ConfigGraficos, Escolha, Sombras};

pub const MOBILE: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Scenes per preset: fight, town, sea.
pub const CENAS: usize = 3;
pub const NOMES_DAS_CENAS: [&str; CENAS] = ["Big fight", "Busy town", "Open sea"];

// ── the player's goal ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objetivo {
    Desempenho,
    Equilibrado,
    Qualidade,
}

impl Objetivo {
    pub const TODOS: [Objetivo; 3] = [Self::Desempenho, Self::Equilibrado, Self::Qualidade];

    pub fn nome(self) -> &'static str {
        match self {
            Self::Desempenho => "Performance",
            Self::Equilibrado => "Balanced",
            Self::Qualidade => "Quality",
        }
    }

    pub fn dica(self) -> &'static str {
        match self {
            Self::Desempenho => "Smoothest play",
            Self::Equilibrado => "A bit of both",
            Self::Qualidade => "Best looks",
        }
    }

    /// (how far the presented p95 may go over the frame budget, how much of
    /// the budget the CPU p95 may use). Performance keeps the most headroom;
    /// Quality accepts the occasional dip.
    pub fn limites(self) -> (f32, f32) {
        match self {
            Self::Desempenho => (1.10, 0.60),
            Self::Equilibrado => (1.25, 0.80),
            Self::Qualidade => (1.50, 0.95),
        }
    }

    fn codigo(self) -> &'static str {
        match self {
            Self::Desempenho => "desempenho",
            Self::Equilibrado => "equilibrado",
            Self::Qualidade => "qualidade",
        }
    }

    fn do_codigo(s: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|o| o.codigo() == s)
    }
}

// ── measuring ────────────────────────────────────────────────────────────

/// One scene at one preset, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Medida {
    /// Average presented frame time (frame to frame).
    pub media_ms: f32,
    /// 95th percentile of the presented frame time.
    pub p95_ms: f32,
    /// 95th percentile of the CPU time spent drawing the scene.
    pub cpu_p95_ms: f32,
}

impl Medida {
    pub fn de(quadros_ms: &[f32], cpu_ms: &[f32]) -> Self {
        let media = if quadros_ms.is_empty() { 0.0 } else { quadros_ms.iter().sum::<f32>() / quadros_ms.len() as f32 };
        Self { media_ms: media, p95_ms: percentil(quadros_ms, 0.95), cpu_p95_ms: percentil(cpu_ms, 0.95) }
    }

    pub fn fps(&self) -> f32 {
        if self.media_ms > 0.0 { 1000.0 / self.media_ms } else { 0.0 }
    }
}

/// Nearest-rank percentile (`p` in 0..1); 0 for an empty list.
pub fn percentil(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let k = ((p * s.len() as f32).ceil() as usize).clamp(1, s.len());
    s[k - 1]
}

fn orcamento_ms(alvo_fps: u16) -> f32 {
    1000.0 / alvo_fps.max(1) as f32
}

/// Does this scene hold `alvo_fps` for this goal? The average must hit the
/// budget, the presented p95 stay within the goal's tolerance, and the CPU
/// leave the goal's headroom (vsync hides GPU headroom, the CPU time shows
/// how close to the edge the frame is).
pub fn passa(m: &Medida, alvo_fps: u16, objetivo: Objetivo) -> bool {
    let t = orcamento_ms(alvo_fps);
    let (tolerancia, folga) = objetivo.limites();
    m.media_ms > 0.0 && m.media_ms <= t * 1.05 && m.p95_ms <= t * tolerancia && m.cpu_p95_ms <= t * folga
}

/// The strictest bar: 60 fps on Performance. A preset that clears it clears
/// every goal and target, so nothing lighter needs measuring.
pub fn passa_em_tudo(medidas: &[Medida]) -> bool {
    medidas.len() == CENAS && medidas.iter().all(|m| passa(m, 60, Objetivo::Desempenho))
}

/// A scene so slow (under ~19 fps) that the preset fails every target: the
/// rest of its scenes are skipped.
pub fn reprova_cedo(m: &Medida) -> bool {
    m.media_ms > orcamento_ms(30) * 1.6
}

/// Enough frames of one scene? At least `min`, at most `max`, and in between
/// as soon as the answer is obvious either way.
pub fn medicao_decidida(quadros_ms: &[f32], cpu_ms: &[f32], min: usize, max: usize) -> bool {
    let n = quadros_ms.len();
    if n >= max {
        return true;
    }
    if n < min {
        return false;
    }
    let m = Medida::de(quadros_ms, cpu_ms);
    let folgado = m.p95_ms < orcamento_ms(60) * 1.08 && m.cpu_p95_ms < orcamento_ms(60) * 0.4;
    folgado || reprova_cedo(&m)
}

/// What the sweep measured, per preset (index into `config_graficos::PRESETS`).
/// `None` = not measured (lighter than one that passed everything, or heavier
/// than the platform tries); fewer than `CENAS` entries = cut short.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Varredura {
    pub niveis: Vec<Option<Vec<Medida>>>,
}

impl Varredura {
    pub fn nova() -> Self {
        Self { niveis: vec![None; cg::PRESETS.len()] }
    }

    fn passa_nivel(&self, nivel: usize, alvo: u16, objetivo: Objetivo) -> bool {
        self.niveis.get(nivel).and_then(|n| n.as_ref()).is_some_and(|ms| {
            ms.len() == CENAS && ms.iter().all(|m| passa(m, alvo, objetivo))
        })
    }

    /// The measurement to quote for `nivel`: its own, or (when it was not
    /// measured) the nearest heavier one, which it can only beat. The worst
    /// scene of the set.
    pub fn medida_para(&self, nivel: usize) -> Option<(usize, Medida)> {
        (nivel..self.niveis.len()).chain((0..nivel).rev()).find_map(|i| {
            let ms = self.niveis[i].as_ref()?;
            let pior = ms.iter().copied().max_by(|a, b| a.media_ms.total_cmp(&b.media_ms))?;
            Some((i, pior))
        })
    }
}

/// The preset to apply, and whether it really holds the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decisao {
    pub nivel: usize,
    pub alcanca: bool,
}

/// The heaviest preset whose every scene passes for this goal and target.
/// Nothing passes: the lightest, and `alcanca = false` so the screen says so.
pub fn decide(v: &Varredura, alvo: u16, objetivo: Objetivo) -> Decisao {
    (0..v.niveis.len())
        .rev()
        .find(|&i| v.passa_nivel(i, alvo, objetivo))
        .map_or(Decisao { nivel: 0, alcanca: false }, |nivel| Decisao { nivel, alcanca: true })
}

/// The target the choice screen starts on: 60 on desktop; on a phone 60 only
/// when Balanced reaches it above the lightest preset, else 30.
pub fn alvo_sugerido(v: &Varredura, mobile: bool) -> u16 {
    if !mobile {
        return 60;
    }
    let d = decide(v, 60, Objetivo::Equilibrado);
    if d.alcanca && d.nivel >= 1 { 60 } else { 30 }
}

/// The goal highlighted as recommended: Balanced, unless even it ends on the
/// lightest preset without holding the target — then Performance.
pub fn recomendado(v: &Varredura, alvo: u16) -> Objetivo {
    let d = decide(v, alvo, Objetivo::Equilibrado);
    if d.nivel == 0 && !d.alcanca { Objetivo::Desempenho } else { Objetivo::Equilibrado }
}

// ── the saved result ─────────────────────────────────────────────────────

/// What was decided on this device. Its presence is what stops the test from
/// running again (Settings › Graphics can still re-run it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resultado {
    pub nivel: usize,
    pub alvo: u16,
    pub objetivo: Objetivo,
    /// The player skipped: the platform default was applied.
    pub pulado: bool,
    /// The fps measured at the applied preset (0 when skipped).
    pub fps: u16,
}

/// Skip: a safe default. Phones get Balanced (Medium) at 30 fps; desktops
/// High at 60.
pub fn padrao_ao_pular(mobile: bool) -> Resultado {
    if mobile {
        Resultado { nivel: 1, alvo: 30, objetivo: Objetivo::Equilibrado, pulado: true, fps: 0 }
    } else {
        Resultado { nivel: 2, alvo: 60, objetivo: Objetivo::Equilibrado, pulado: true, fps: 0 }
    }
}

pub fn escreve(r: &Resultado) -> String {
    format!(
        "versao=1\nnivel={}\nalvo={}\nobjetivo={}\npulado={}\nfps={}\n",
        r.nivel,
        r.alvo,
        r.objetivo.codigo(),
        r.pulado as u8,
        r.fps
    )
}

/// `None` for a missing, old or broken file: the test then runs again,
/// which is the safe side.
pub fn le(texto: &str) -> Option<Resultado> {
    let mut nivel = None;
    let mut alvo = None;
    let mut objetivo = None;
    let mut pulado = false;
    let mut fps = 0;
    let mut versao = false;
    for linha in texto.lines() {
        let Some((k, v)) = linha.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "versao" => versao = v == "1",
            "nivel" => nivel = v.parse::<usize>().ok().filter(|n| *n < cg::PRESETS.len()),
            "alvo" => alvo = v.parse::<u16>().ok().filter(|a| *a == 30 || *a == 60),
            "objetivo" => objetivo = Objetivo::do_codigo(v),
            "pulado" => pulado = v == "1",
            "fps" => fps = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    if !versao {
        return None;
    }
    Some(Resultado { nivel: nivel?, alvo: alvo?, objetivo: objetivo?, pulado, fps })
}

/// Whether the test has to run, given the saved file's text.
pub fn precisa_rodar(texto: Option<&str>) -> bool {
    texto.and_then(le).is_none()
}

fn arquivo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_file_name("benchmark.prefs"))
}

pub fn salva_em(p: &std::path::Path, r: &Resultado) -> std::io::Result<()> {
    std::fs::write(p, escreve(r))
}

pub fn carrega_de(p: &std::path::Path) -> Option<Resultado> {
    std::fs::read_to_string(p).ok().as_deref().and_then(le)
}

fn salva(r: &Resultado) {
    if let Some(p) = arquivo() {
        let _ = salva_em(&p, r);
    }
}

/// Applies a decision and remembers it on the device.
fn aplica(cfg: &mut ConfigGraficos, r: &Resultado) {
    cfg.republica();
    cfg.aplica_nivel(r.nivel, r.alvo);
    salva(r);
}

// ── the runtime guard ────────────────────────────────────────────────────

/// Far under target: below this fraction of the target fps.
pub const LIMIAR: f32 = 0.7;
/// For this long, in seconds, before offering to step down.
pub const ESPERA_S: f64 = 20.0;
/// Measured in windows of this length.
const JANELA_S: f64 = 1.0;
/// "Not now" keeps it quiet this long.
pub const SILENCIO_RECUSA_S: f64 = 600.0;

/// In game: if the fps stays far under target for more than `ESPERA_S`, the
/// HUD offers (without blocking anything) to step the graphics down one
/// preset. Never when there is no lighter preset to go to.
#[derive(Default)]
pub struct Guarda {
    janela: Option<(f64, u32)>,
    abaixo_desde: Option<f64>,
    silencio_ate: f64,
    pub oferta: bool,
    /// Where the offer was drawn: taps there are not the world's.
    pub(crate) area: Option<Rect>,
}

/// The fps the guard holds the game to: the frame-rate cap, or 60 with none.
pub fn alvo_da_guarda(limite_fps: u16) -> f32 {
    if limite_fps == 0 { 60.0 } else { limite_fps as f32 }
}

impl Guarda {
    /// One frame. `pode_baixar`: a lighter preset exists. Without one the
    /// offer never shows, however slow the game runs.
    pub fn observa(&mut self, agora: f64, dt: f32, alvo_fps: f32, pode_baixar: bool) {
        if !pode_baixar {
            self.oferta = false;
            self.abaixo_desde = None;
            self.janela = None;
            return;
        }
        // A hitch (loading, the app coming back from the background) is not
        // the steady frame rate: start the window over.
        if dt > 0.5 {
            self.janela = None;
            return;
        }
        let (inicio, quadros) = self.janela.get_or_insert((agora, 0));
        *quadros += 1;
        let dur = agora - *inicio;
        if dur >= JANELA_S {
            let fps = *quadros as f64 / dur;
            let inicio = *inicio;
            if fps < (alvo_fps * LIMIAR) as f64 {
                self.abaixo_desde.get_or_insert(inicio);
            } else {
                self.abaixo_desde = None;
            }
            self.janela = Some((agora, 0));
        }
        if let Some(d) = self.abaixo_desde {
            if agora - d >= ESPERA_S && agora >= self.silencio_ate {
                self.oferta = true;
            }
        }
    }

    /// Not observing this frame (loading, battery saver, a panel): forget the
    /// streak, keep the offer as it is.
    pub fn pausa(&mut self) {
        self.janela = None;
        self.abaixo_desde = None;
    }

    pub fn recusou(&mut self, agora: f64) {
        self.oferta = false;
        self.abaixo_desde = None;
        self.silencio_ate = agora + SILENCIO_RECUSA_S;
    }

    pub fn baixou(&mut self) {
        self.oferta = false;
        self.abaixo_desde = None;
        self.janela = None;
    }

    pub fn pega(&self, p: Vec2) -> bool {
        self.oferta && self.area.is_some_and(|r| r.contains(p))
    }
}

// ── the run ──────────────────────────────────────────────────────────────

/// Frames thrown away after a scene or preset change: the first draws pay
/// for chunk uploads, shader variants and the damage numbers' glyphs.
const AQUECE: u32 = 15;
/// Frames measured per scene: (minimum, maximum). Desktop is shorter.
const QUADROS: (usize, usize) = if MOBILE { (30, 75) } else { (20, 45) };
/// Chunks baked per frame while a scene is built.
const ORCAMENTO: usize = if MOBILE { 6 } else { 16 };
/// Whatever happens, the test ends after this many seconds.
const TETO_S: f64 = 45.0;
/// The presets tried, heaviest first. Ultra stays out on phones: its view
/// distance alone is ~400 MB of mesh, which the character screen should not
/// risk on a small phone (the panel still offers it).
fn plano() -> Vec<usize> {
    let topo = if MOBILE { 2 } else { cg::PRESETS.len() - 1 };
    (0..=topo).rev().collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Etapa {
    Construindo,
    Aquecendo(u32),
    Gravando,
}

pub(crate) struct Teste {
    plano: Vec<usize>,
    i: usize,
    cena: usize,
    etapa: Etapa,
    quadros: Vec<f32>,
    cpu: Vec<f32>,
    anterior_gravado: bool,
    pub(crate) varredura: Varredura,
    ilha: Option<crate::terreno::Terreno>,
    mar: Option<crate::terreno::Terreno>,
    casas: crate::construcoes::Construcoes,
    mundo: crate::world::World,
    luz: Option<Material>,
    inicio: f64,
    centro: Vec2,
    mar_alvo: Vec2,
    mar_rumo: Vec2,
    relogio: f32,
    proxima_dano: f32,
    proximo_passo: f32,
}

/// The Plateau: the heaviest island the client builds alone (dense ground
/// cover, trees, the town and the fort).
fn ilha_pesada() -> &'static shared::terreno::DefIlha {
    &shared::terreno::ARQUIPELAGO[3]
}

pub(crate) const KINDS_DO_PLANALTO: [u16; 6] = [36, 37, 38, 39, 40, 15];
const INIMIGOS: u32 = 32;
const OUTROS_JOGADORES: u32 = 6;

impl Teste {
    fn novo() -> Self {
        let def = ilha_pesada();
        let g = shared::terreno::Gerador::da_ilha(def);
        let c = g.cidade().map(|c| c.centro()).unwrap_or_default();
        let (mar_alvo, mar_rumo) = match shared::mar::vista_da_zona("ilha_inicial") {
            Some(v) => {
                let f = v.fundeadouro();
                let r = (v.ponta - v.raiz).normalize_or_zero();
                (vec2(f.x + r.x * 90.0, f.y + r.y * 90.0), vec2(r.x, r.y))
            }
            None => (Vec2::ZERO, Vec2::Y),
        };
        let mut t = Self {
            plano: plano(),
            i: 0,
            cena: 0,
            etapa: Etapa::Construindo,
            quadros: Vec::new(),
            cpu: Vec::new(),
            anterior_gravado: false,
            varredura: Varredura::nova(),
            ilha: None,
            mar: None,
            casas: crate::construcoes::Construcoes::para(Some(def)),
            mundo: crate::world::World::default(),
            luz: None,
            inicio: get_time(),
            centro: vec2(c.x, c.y),
            mar_alvo,
            mar_rumo,
            relogio: 0.0,
            proxima_dano: 0.0,
            proximo_passo: 0.0,
        };
        t.povoa();
        t
    }

    fn nivel(&self) -> usize {
        self.plano[self.i]
    }

    fn escolha(&self) -> Escolha {
        cg::preset(self.nivel())
    }

    /// 0..1 for the bar.
    pub(crate) fn progresso(&self) -> f32 {
        let total = (self.plano.len() * CENAS) as f32;
        let parcial = match self.etapa {
            Etapa::Gravando => self.quadros.len() as f32 / QUADROS.1 as f32,
            _ => 0.0,
        };
        ((self.i * CENAS + self.cena) as f32 + parcial.min(1.0)) / total
    }

    /// "Big fight · High", in the player's language.
    pub(crate) fn rotulo(&self) -> String {
        format!(
            "{} · {}",
            shared::idioma::tr(NOMES_DAS_CENAS[self.cena]),
            shared::idioma::tr(cg::PRESETS[self.nivel()])
        )
    }

    /// The crowd: the player with a rebirth aura, others in full gear auras,
    /// and a ring of the Plateau's monsters.
    fn povoa(&mut self) {
        use shared::{EntityId, EntityMeta, EntityState, EntityTag};
        let mut metas = Vec::new();
        let mut estados = Vec::new();
        let combate = shared::components::acao::monta(0, true, 0, 0);
        let meta = |id: u32, tag, kind, auras, renascimentos| EntityMeta {
            skins: 0,
            cla: String::new(),
            pk: Default::default(),
            auras,
            id: EntityId(id),
            tag,
            name: None,
            hp_max: 1000,
            faction: None,
            kind,
            nivel: 80,
            desafio: None,
            aparencia: 0,
            poder: 0,
            renascimentos,
        };
        // Me: the black-hole rebirth aura (the heaviest).
        metas.push(meta(1, EntityTag::Player, 0, 0, 15));
        let mut eu = EntityState::quantize(EntityId(1), ::glam::Vec2::new(self.centro.x, self.centro.y), ::glam::Vec2::ZERO, 1000, shared::ent_flags::SELF);
        eu.acao = combate;
        estados.push(eu);
        for k in 0..OUTROS_JOGADORES {
            let tier = (k % 4) as u64;
            let bits = (0..shared::auras::SLOTS).fold(0u64, |b, s| b | ((3 | (tier << 3)) << (s * 8)));
            metas.push(meta(10 + k, EntityTag::Player, 0, bits, 0));
        }
        for k in 0..INIMIGOS {
            metas.push(meta(100 + k, EntityTag::Enemy, KINDS_DO_PLANALTO[k as usize % KINDS_DO_PLANALTO.len()], 0, 0));
        }
        estados.extend(self.posicoes(0.0).into_iter().skip(1));
        self.mundo.apply(metas, estados, &[]);
    }

    /// Everyone but me on slowly turning rings, so the bodies walk.
    fn posicoes(&self, t: f32) -> Vec<shared::EntityState> {
        use shared::{EntityId, EntityState};
        let combate = shared::components::acao::monta(0, true, 0, 0);
        let c = self.centro;
        let mut v = vec![EntityState::quantize(EntityId(1), ::glam::Vec2::new(c.x, c.y), ::glam::Vec2::ZERO, 1000, shared::ent_flags::SELF)];
        let em = |id: u32, raio: f32, a: f32, vel: f32, flags: u8| {
            let p = ::glam::Vec2::new(c.x + a.cos() * raio, c.y + a.sin() * raio);
            let d = ::glam::Vec2::new(-a.sin(), a.cos()) * vel;
            let mut s = EntityState::quantize(EntityId(id), p, d, 800, flags);
            s.acao = combate;
            s
        };
        for k in 0..OUTROS_JOGADORES {
            let a = k as f32 / OUTROS_JOGADORES as f32 * std::f32::consts::TAU + t * 0.15;
            v.push(em(10 + k, 2.6, a, 0.4, 0));
        }
        for k in 0..INIMIGOS {
            let anel = 4.5 + (k % 3) as f32 * 1.8;
            let a = k as f32 * 2.399 + t * 0.25;
            v.push(em(100 + k, anel, a, 1.2, shared::ent_flags::ATACANDO));
        }
        v
    }

    /// For the preview: a shot once each scene is a few frames into its
    /// measurement.
    #[cfg(debug_assertions)]
    pub(crate) fn chave_da_foto(&self) -> Option<String> {
        (self.etapa == Etapa::Gravando && self.quadros.len() == 10)
            .then(|| format!("{}-{}", cg::PRESETS[self.nivel()].to_lowercase(), self.cena))
    }

    /// One frame of the test. `true` when it is over.
    pub(crate) fn passo(&mut self, vox: &crate::vox::VoxCache, solido: &Material) -> bool {
        if get_time() - self.inicio > TETO_S {
            return true;
        }
        let e = self.escolha();
        match self.etapa {
            Etapa::Construindo => {
                cg::publica_para_teste(&e);
                self.casas.acompanhar();
                let no_mar = self.cena == 2;
                let (alvo, t) = if no_mar {
                    self.ilha = None;
                    (self.mar_alvo, self.mar.get_or_insert_with(|| crate::terreno::Terreno::novo(&shared::mar::DEF)))
                } else {
                    self.mar = None;
                    (self.centro, self.ilha.get_or_insert_with(|| crate::terreno::Terreno::novo(ilha_pesada())))
                };
                t.atualiza(alvo, e.raio, ORCAMENTO);
                let (feitos, total) = t.prontos_em(alvo, e.raio);
                crate::ui::fundo();
                if feitos >= total && self.casas.prontas() {
                    self.etapa = Etapa::Aquecendo(0);
                }
            }
            Etapa::Aquecendo(k) => {
                self.desenha_cena(vox, solido, &e);
                self.etapa = if k + 1 >= AQUECE { Etapa::Gravando } else { Etapa::Aquecendo(k + 1) };
                self.quadros.clear();
                self.cpu.clear();
                self.anterior_gravado = false;
            }
            Etapa::Gravando => {
                if self.anterior_gravado {
                    self.quadros.push(get_frame_time() * 1000.0);
                }
                let t0 = std::time::Instant::now();
                self.desenha_cena(vox, solido, &e);
                unsafe { get_internal_gl().flush() };
                self.cpu.push(t0.elapsed().as_secs_f32() * 1000.0);
                self.anterior_gravado = true;
                if medicao_decidida(&self.quadros, &self.cpu, QUADROS.0, QUADROS.1) {
                    return self.fecha_cena();
                }
            }
        }
        false
    }

    /// A scene is measured: on to the next scene, preset, or the end.
    fn fecha_cena(&mut self) -> bool {
        let m = Medida::de(&self.quadros, &self.cpu);
        let nivel = self.nivel();
        let lista = self.varredura.niveis[nivel].get_or_insert_with(Vec::new);
        lista.push(m);
        let completo = lista.len() == CENAS;
        let tudo = passa_em_tudo(lista);
        if !reprova_cedo(&m) && !completo {
            self.cena += 1;
            // The sea is another terrain; fight and town share the island.
            self.etapa = if self.cena == 2 { Etapa::Construindo } else { Etapa::Aquecendo(0) };
            return false;
        }
        if tudo || self.i + 1 >= self.plano.len() {
            return true;
        }
        self.i += 1;
        self.cena = 0;
        // A new preset: fresh terrain at its view distance and ground cover.
        self.ilha = None;
        self.mar = None;
        self.etapa = Etapa::Construindo;
        false
    }

    fn desenha_cena(&mut self, vox: &crate::vox::VoxCache, solido: &Material, e: &Escolha) {
        let dt = get_frame_time().min(0.1);
        self.relogio += dt;
        match self.cena {
            0 => self.desenha_ilha(vox, solido, e, false),
            1 => self.desenha_ilha(vox, solido, e, true),
            _ => {
                if let Some(t) = &self.mar {
                    cena_mar(t, solido, self.mar_alvo, self.mar_rumo, e);
                }
            }
        }
    }

    /// Scene (a), the fight (`longe = false`), and (b), the town at the
    /// farthest zoom this view distance allows (`longe = true`). Same island,
    /// same crowd; the fight adds skill effects and damage numbers.
    fn desenha_ilha(&mut self, vox: &crate::vox::VoxCache, solido: &Material, e: &Escolha, longe: bool) {
        let Some(t) = &self.ilha else { return };
        let dt = get_frame_time().min(0.1);
        let mut perfil = Perfil::novo();
        // The crowd moves and swings.
        if self.relogio >= self.proximo_passo {
            self.proximo_passo = self.relogio + 0.25;
            let estados = self.posicoes(self.relogio);
            self.mundo.apply(Vec::new(), estados, &[]);
        }
        self.mundo.tick(dt, &|x, z, _| t.altura(x, z));
        for (id, ent) in self.mundo.ents.iter_mut() {
            ent.sacada = 1.0;
            if ent.meta.tag == shared::EntityTag::Enemy && ent.golpe > 0.9 + (id.0 % 5) as f32 * 0.2 {
                ent.golpe = 0.0;
            }
        }
        if !longe && self.relogio >= self.proxima_dano {
            self.proxima_dano = self.relogio + 0.07;
            let k = (self.relogio * 1000.0) as u32;
            self.mundo.efeitos.push(crate::world::Efeito {
                alvo: shared::EntityId(100 + k % INIMIGOS),
                dano: 900 + (k * 37 % 4000) as i32,
                critico: k % 4 == 0,
                eu: false,
                t: 0.0,
                semente: (k % 1000) as f32 / 1000.0,
            });
        }
        let chao = |x: f32, z: f32| t.altura(x, z);
        let apoio = t.altura(self.centro.x, self.centro.y);
        let (zoom, pitch) = if longe {
            let z = crate::render3d::ZOOM_MAX;
            (z, crate::render3d::pitch_min_com_raio(z, e.raio))
        } else {
            (1.0, crate::render3d::pitch_do_zoom(1.0))
        };
        let vista = crate::render3d::Vista::nova(self.centro, 0.6 + self.relogio * 0.05, zoom, pitch, apoio, &chao);
        perfil.marca("tick");
        crate::render3d::clear();
        set_camera(&vista.cam);
        luz_e_neblina(solido, e, self.centro);
        gl_use_material(solido);
        solido.set_uniform("Crop", Vec3::ZERO);
        solido.set_uniform("RecorteZ", 0.0f32);
        t.desenha(&vista.cam, Vec3::ZERO, 0.0);
        let eu = vec3(self.centro.x, apoio, self.centro.y);
        self.casas.desenha(&vista.cam, Some(eu), Vec3::ZERO, 0.0);
        if e.sombras == Sombras::Bonitas {
            t.desenha_sombras(&vista.cam);
        }
        crate::agua::desenha(t, &vista.cam, get_time() as f32);
        perfil.marca("cenario");
        gl_use_material(solido);
        crate::render3d::draw_entities_com_sombras(&mut self.mundo, vox, None, &vista, e.sombras);
        perfil.marca("corpos");
        apaga_luz_e_neblina(solido);
        gl_use_default_material();
        if !longe {
            let luz = self.luz.get_or_insert_with(crate::habilidades_vfx::material);
            // Mine always; the other players' only with their effects on.
            let lancadores = if e.efeitos_dos_outros { 1 + OUTROS_JOGADORES } else { 1 };
            for k in 0..lancadores {
                let id = if k == 0 { 1 } else { 10 + k - 1 };
                let Some(de) = self.mundo.ents.get(&shared::EntityId(id)).map(|x| x.render_pos) else { continue };
                let Some(alvo) = self.mundo.ents.get(&shared::EntityId(100 + (k * 5) % INIMIGOS)).map(|x| x.render_pos) else { continue };
                let de3 = vec3(de.x, t.altura(de.x, de.y), de.y);
                let alvo3 = vec3(alvo.x, t.altura(alvo.x, alvo.y), alvo.y);
                let skill = 1 + (k % 12);
                let ciclo = crate::habilidades_vfx::duracao(skill) + 0.4;
                let fase = (self.relogio + k as f32 * 0.37) % ciclo;
                let frente = vec3(alvo3.x - de3.x, 0.0, alvo3.z - de3.z).normalize_or_zero();
                let cena = |impacto: bool, tt: f32| crate::habilidades_vfx::Cena {
                    id: skill,
                    de: de3,
                    alvo: alvo3,
                    maos: [de3 + Vec3::Y * 1.2; 2],
                    t: tt,
                    impacto,
                    atraso: 0.4,
                    raio: 3.0,
                    frente,
                    tier: 10,
                };
                crate::habilidades_vfx::desenha(&cena(false, fase.min(0.9)), luz);
                if fase > 0.4 {
                    crate::habilidades_vfx::desenha(&cena(true, fase - 0.4), luz);
                }
            }
            gl_use_default_material();
        }
        perfil.marca("vfx");
        set_default_camera();
        if !longe {
            crate::efeitos::desenha(&self.mundo, &vista);
        }
        perfil.marca("numeros");
    }
}

/// `MMO_BENCH_PERFIL=1` (debug): prints the sections of a slow scene frame.
struct Perfil {
    #[cfg(debug_assertions)]
    t: std::time::Instant,
    #[cfg(debug_assertions)]
    partes: Vec<(&'static str, f32)>,
    #[cfg(debug_assertions)]
    ligado: bool,
}

impl Perfil {
    fn novo() -> Self {
        Self {
            #[cfg(debug_assertions)]
            t: std::time::Instant::now(),
            #[cfg(debug_assertions)]
            partes: Vec::new(),
            #[cfg(debug_assertions)]
            ligado: std::env::var("MMO_BENCH_PERFIL").is_ok(),
        }
    }

    #[allow(unused_variables)]
    fn marca(&mut self, nome: &'static str) {
        #[cfg(debug_assertions)]
        if self.ligado {
            unsafe { get_internal_gl().flush() };
            self.partes.push((nome, self.t.elapsed().as_secs_f32() * 1000.0));
            self.t = std::time::Instant::now();
            if nome == "numeros" {
                let total: f32 = self.partes.iter().map(|p| p.1).sum();
                if total > 30.0 {
                    println!("[benchmark perfil] {total:.1} ms: {:?}", self.partes);
                }
            }
        }
    }
}

fn luz_e_neblina(solido: &Material, e: &Escolha, centro: Vec2) {
    let luz = crate::render3d::luz_dia(if e.sombras == Sombras::Bonitas { 1.0 } else { 0.0 });
    solido.set_uniform("LuzDia", luz);
    crate::gpu_estatica::define_luz_dia(luz);
    let (inicio, fim) = cg::neblina();
    crate::gpu_estatica::define_neblina(centro, inicio, fim);
    solido.set_uniform("Neblina", vec4(centro.x, centro.y, inicio, fim));
}

/// Back to what the screens around expect (the character preview uses the
/// same shader and must not fog).
fn apaga_luz_e_neblina(solido: &Material) {
    solido.set_uniform("LuzDia", 0.0f32);
    crate::gpu_estatica::define_luz_dia(0.0);
    solido.set_uniform("Neblina", Vec4::ZERO);
    crate::gpu_estatica::define_neblina(Vec2::ZERO, 0.0, 0.0);
}

/// Scene (c): SAILING. Open water off the Grove's dock with a small fleet,
/// drawn by the sea's own scene builder (`mar_cena::desenha_mundo`: the
/// islands' far meshes, the dock with its lighthouse and boats, the sea
/// shader, the sky, wreckage, and the hulls with sails, rigging and wake), so
/// the test measures exactly what sailing costs. The sea sets its own fog
/// and light (`mar_cena::neblina`); `e` is the same for every scene.
pub fn cena_mar(t: &crate::terreno::Terreno, solido: &Material, alvo: Vec2, rumo: Vec2, e: &Escolha) {
    let _ = e;
    let costas = -rumo;
    let (recuo, alto) = (40.0, 22.0);
    let cam = Camera3D {
        position: vec3(alvo.x + costas.x * recuo * 0.6 + 6.0, alto, alvo.y + costas.y * recuo * 0.6 + recuo * 0.4),
        target: vec3(alvo.x, 0.0, alvo.y),
        up: Vec3::Y,
        fovy: 0.9,
        ..Default::default()
    };
    clear_background(crate::mar_cena::cor_de_fundo());
    let frota: Vec<(Vec2, f32, shared::navio::Visual, u64)> = [(0.0f32, 0.0f32, 0u8), (8.0, 5.0, 1), (-6.0, 9.0, 3), (14.0, -4.0, 2), (-12.0, -2.0, 1), (3.0, 16.0, 0)]
        .into_iter()
        .enumerate()
        .map(|(k, (dx, dz, classe))| {
            let v = shared::navio::Visual { classe, vela: classe + 1, ..Default::default() };
            (vec2(alvo.x + dx, alvo.y + dz), rumo.x.atan2(rumo.y) + k as f32 * 0.6, v, k as u64)
        })
        .collect();
    crate::mar_cena::desenha_mundo(t, solido, &cam, &frota, Some(alvo));
    apaga_luz_e_neblina(solido);
    set_default_camera();
}

// ── the state machine `main` drives ──────────────────────────────────────

pub(crate) enum Fase {
    Parado,
    Medindo(Box<Teste>),
    Escolha(crate::benchmark_ui::EscolhaUi),
}

pub struct Benchmark {
    pub(crate) fase: Fase,
    /// The device has no saved result: run on the next character screen.
    precisa: bool,
    pub guarda: Guarda,
}

impl Default for Benchmark {
    fn default() -> Self {
        // Automated runs and previews never stop for it.
        let desligado = ["MMO_SEM_BENCHMARK", "MMO_CHAR", "MMO_PREVIA_LOGIN"]
            .iter()
            .any(|v| std::env::var(v).is_ok_and(|s| !s.is_empty()));
        let texto = arquivo().and_then(|p| std::fs::read_to_string(p).ok());
        Self { fase: Fase::Parado, precisa: !desligado && precisa_rodar(texto.as_deref()), guarda: Guarda::default() }
    }
}

impl Benchmark {
    pub fn ativo(&self) -> bool {
        !matches!(self.fase, Fase::Parado)
    }

    /// Measuring right now: the frame-rate caps must stay off.
    pub fn medindo(&self) -> bool {
        matches!(self.fase, Fase::Medindo(_))
    }

    /// On the character screen: starts the first-time test when due.
    pub fn quer_rodar(&mut self) -> bool {
        if std::mem::take(&mut self.precisa) {
            self.iniciar();
        }
        self.ativo()
    }

    pub fn iniciar(&mut self) {
        self.fase = Fase::Medindo(Box::new(Teste::novo()));
    }

    /// Taps on the benchmark's own UI are not the world's.
    pub fn pega(&self, p: Vec2) -> bool {
        self.ativo() || self.guarda.pega(p)
    }

    /// Draws one frame of the test or of the choice screen, in place of the
    /// screen under it.
    pub fn quadro(&mut self, vox: &crate::vox::VoxCache, solido: &Material, cfg: &mut ConfigGraficos) {
        match &mut self.fase {
            Fase::Parado => {}
            Fase::Medindo(t) => {
                let acabou = t.passo(vox, solido);
                let pulou = crate::benchmark_ui::desenha_progresso(t.progresso(), &t.rotulo());
                if pulou {
                    aplica(cfg, &padrao_ao_pular(MOBILE));
                    self.fase = Fase::Parado;
                } else if acabou {
                    cfg.republica();
                    apaga_luz_e_neblina(solido);
                    let v = std::mem::take(&mut t.varredura);
                    self.fase = Fase::Escolha(crate::benchmark_ui::EscolhaUi::nova(v, MOBILE));
                }
            }
            Fase::Escolha(e) => {
                if let Some(r) = crate::benchmark_ui::desenha_escolha(e) {
                    aplica(cfg, &r);
                    self.fase = Fase::Parado;
                }
            }
        }
    }

    /// In game, once per frame: the slow-frame guard and its offer.
    /// `observar`: a steady, normal frame (no loading, no battery saver).
    pub fn guarda_quadro(&mut self, cfg: &mut ConfigGraficos, observar: bool) {
        let agora = get_time();
        if observar {
            self.guarda.observa(agora, get_frame_time(), alvo_da_guarda(cg::limite_fps()), cfg.pode_baixar());
        } else {
            self.guarda.pausa();
        }
        if !self.guarda.oferta {
            self.guarda.area = None;
            return;
        }
        match crate::benchmark_ui::desenha_guarda(&mut self.guarda) {
            Some(true) => {
                cfg.baixar_um_nivel();
                self.guarda.baixou();
            }
            Some(false) => self.guarda.recusou(agora),
            None => {}
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn m(media: f32, p95: f32, cpu: f32) -> Medida {
        Medida { media_ms: media, p95_ms: p95, cpu_p95_ms: cpu }
    }

    fn varredura(niveis: &[Option<[Medida; 3]>]) -> Varredura {
        Varredura { niveis: niveis.iter().map(|n| n.map(|a| a.to_vec())).collect() }
    }

    const RAPIDO: Medida = Medida { media_ms: 16.7, p95_ms: 17.0, cpu_p95_ms: 6.0 };
    const JUSTO_60: Medida = Medida { media_ms: 16.8, p95_ms: 18.0, cpu_p95_ms: 12.0 };
    const A_30: Medida = Medida { media_ms: 31.0, p95_ms: 34.0, cpu_p95_ms: 12.0 };
    const LENTO: Medida = Medida { media_ms: 70.0, p95_ms: 90.0, cpu_p95_ms: 50.0 };

    #[test]
    fn os_presets_vao_do_mais_leve_ao_mais_pesado() {
        let mut ant = cg::preset(0);
        for i in 1..cg::PRESETS.len() {
            let p = cg::preset(i);
            assert!(p.raio >= ant.raio && p.forracao >= ant.forracao, "{i}");
            assert!(p.sombras as u8 >= ant.sombras as u8 && p.ondas >= ant.ondas, "{i}");
            assert!(p != ant, "{i} igual ao anterior");
            ant = p;
        }
        // The sweep goes heaviest first, and phones never try Ultra.
        let pl = plano();
        assert!(pl.windows(2).all(|w| w[0] > w[1]));
        assert_eq!(*pl.last().unwrap(), 0);
        assert_eq!(pl[0], if MOBILE { 2 } else { cg::PRESETS.len() - 1 });
    }

    #[test]
    fn percentil_e_medida() {
        let v: Vec<f32> = (1..=100).map(|x| x as f32).collect();
        assert_eq!(percentil(&v, 0.95), 95.0);
        assert_eq!(percentil(&[], 0.95), 0.0);
        let md = Medida::de(&[10.0, 20.0], &[5.0]);
        assert_eq!(md.media_ms, 15.0);
        assert!((md.fps() - 66.67).abs() < 0.1);
    }

    #[test]
    fn a_regra_depende_do_objetivo_e_do_alvo() {
        // 60 with some CPU headroom: Balanced and Quality yes, Performance no.
        assert!(!passa(&JUSTO_60, 60, Objetivo::Desempenho));
        assert!(passa(&JUSTO_60, 60, Objetivo::Equilibrado));
        assert!(passa(&JUSTO_60, 60, Objetivo::Qualidade));
        assert!(passa(&JUSTO_60, 30, Objetivo::Desempenho));
        // Steady 30 fails 60 everywhere and passes 30.
        for o in Objetivo::TODOS {
            assert!(!passa(&A_30, 60, o));
            assert!(passa(&A_30, 30, o), "{o:?}");
        }
        assert!(!passa(&LENTO, 30, Objetivo::Qualidade));
    }

    #[test]
    fn escolhe_o_mais_pesado_que_passa() {
        // Ultra at 30, High just at 60, Medium fast.
        let v = varredura(&[None, Some([RAPIDO; 3]), Some([JUSTO_60, RAPIDO, JUSTO_60]), Some([A_30; 3])]);
        assert_eq!(decide(&v, 60, Objetivo::Qualidade), Decisao { nivel: 2, alcanca: true });
        assert_eq!(decide(&v, 60, Objetivo::Equilibrado), Decisao { nivel: 2, alcanca: true });
        assert_eq!(decide(&v, 60, Objetivo::Desempenho), Decisao { nivel: 1, alcanca: true });
        assert_eq!(decide(&v, 30, Objetivo::Desempenho), Decisao { nivel: 3, alcanca: true });
        // A preset cut short (one scene) never wins.
        let mut v2 = v.clone();
        v2.niveis[3] = Some(vec![RAPIDO]);
        assert_eq!(decide(&v2, 30, Objetivo::Desempenho).nivel, 2);
        // Friendly line: the worst scene of the decided preset.
        assert_eq!(v.medida_para(2).unwrap(), (2, JUSTO_60));
        // Unmeasured Low: quote the nearest heavier one.
        assert_eq!(v.medida_para(0).unwrap().0, 1);
    }

    #[test]
    fn nada_passa_cai_no_mais_leve() {
        let v = varredura(&[Some([LENTO; 3]), Some([LENTO; 3]), Some([LENTO; 3]), None]);
        for o in Objetivo::TODOS {
            assert_eq!(decide(&v, 30, o), Decisao { nivel: 0, alcanca: false });
        }
        assert_eq!(recomendado(&v, 30), Objetivo::Desempenho);
        assert_eq!(alvo_sugerido(&v, true), 30);
        assert_eq!(decide(&Varredura::nova(), 60, Objetivo::Equilibrado), Decisao { nivel: 0, alcanca: false });
    }

    #[test]
    fn alvo_e_recomendacao_sugeridos() {
        let forte = varredura(&[None, None, Some([RAPIDO; 3]), None]);
        assert_eq!(alvo_sugerido(&forte, true), 60);
        assert_eq!(recomendado(&forte, 60), Objetivo::Equilibrado);
        let fraco = varredura(&[Some([A_30; 3]), Some([A_30; 3]), Some([A_30; 3]), None]);
        assert_eq!(alvo_sugerido(&fraco, true), 30);
        assert_eq!(alvo_sugerido(&fraco, false), 60);
        assert!(passa_em_tudo(&[RAPIDO; 3]) && !passa_em_tudo(&[RAPIDO; 2]) && !passa_em_tudo(&[JUSTO_60; 3]));
    }

    #[test]
    fn a_medicao_para_cedo_quando_e_obvio() {
        let rapido = vec![16.7; 20];
        assert!(!medicao_decidida(&rapido[..10], &[3.0; 10], 20, 45));
        assert!(medicao_decidida(&rapido, &[3.0; 20], 20, 45));
        assert!(!medicao_decidida(&vec![25.0; 20], &[10.0; 20], 20, 45));
        assert!(medicao_decidida(&vec![80.0; 20], &[10.0; 20], 20, 45));
        assert!(medicao_decidida(&vec![25.0; 45], &[10.0; 45], 20, 45));
        assert!(reprova_cedo(&LENTO) && !reprova_cedo(&A_30));
    }

    #[test]
    fn o_resultado_fica_salvo_e_nao_roda_de_novo() {
        let r = Resultado { nivel: 2, alvo: 60, objetivo: Objetivo::Qualidade, pulado: false, fps: 57 };
        assert_eq!(le(&escreve(&r)), Some(r));
        assert!(precisa_rodar(None));
        assert!(precisa_rodar(Some("")));
        assert!(precisa_rodar(Some("nivel=9\nalvo=60\n")));
        assert!(!precisa_rodar(Some(&escreve(&r))));
        // Through a real file.
        let dir = std::env::temp_dir().join(format!("tempest-bench-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("benchmark.prefs");
        assert!(carrega_de(&p).is_none());
        salva_em(&p, &r).unwrap();
        assert_eq!(carrega_de(&p), Some(r));
        let texto = std::fs::read_to_string(&p).unwrap();
        assert!(!precisa_rodar(Some(&texto)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pular_aplica_o_padrao_da_plataforma() {
        let celular = padrao_ao_pular(true);
        assert_eq!((celular.nivel, celular.alvo, celular.objetivo), (1, 30, Objetivo::Equilibrado));
        assert_eq!(cg::PRESETS[celular.nivel], "Medium");
        assert!(celular.pulado);
        let pc = padrao_ao_pular(false);
        assert_eq!((pc.nivel, pc.alvo), (2, 60));
        // Skipping also counts as done.
        assert!(!precisa_rodar(Some(&escreve(&celular))));
    }

    /// Simulates `segundos` of play at `fps`.
    fn joga(g: &mut Guarda, desde: f64, segundos: f64, fps: f64, alvo: f32, pode_baixar: bool) -> f64 {
        let dt = 1.0 / fps;
        let mut t = desde;
        while t < desde + segundos {
            t += dt;
            g.observa(t, dt as f32, alvo, pode_baixar);
        }
        t
    }

    #[test]
    fn a_guarda_espera_vinte_segundos_bem_abaixo() {
        let mut g = Guarda::default();
        // 40 fps against 60 is under 70% (42): offered only after 20 s.
        let t = joga(&mut g, 0.0, 15.0, 40.0, 60.0, true);
        assert!(!g.oferta);
        let t = joga(&mut g, t, 7.0, 40.0, 60.0, true);
        assert!(g.oferta);
        // "Not now": quiet for ten minutes even if it stays slow.
        g.recusou(t);
        let t = joga(&mut g, t, 60.0, 40.0, 60.0, true);
        assert!(!g.oferta);
        let _ = t;
        // 45 fps against 60 is not far under: never offered.
        let mut g = Guarda::default();
        joga(&mut g, 0.0, 40.0, 45.0, 60.0, true);
        assert!(!g.oferta);
        // 30 cap: 25 fps is fine, 18 is far under.
        let mut g = Guarda::default();
        joga(&mut g, 0.0, 30.0, 25.0, alvo_da_guarda(30), true);
        assert!(!g.oferta);
        joga(&mut g, 30.0, 25.0, 18.0, alvo_da_guarda(30), true);
        assert!(g.oferta);
        assert_eq!(alvo_da_guarda(0), 60.0);
    }

    #[test]
    fn a_guarda_zera_quando_melhora_ou_engasga() {
        let mut g = Guarda::default();
        let t = joga(&mut g, 0.0, 15.0, 30.0, 60.0, true);
        let t = joga(&mut g, t, 3.0, 60.0, 60.0, true);
        joga(&mut g, t, 15.0, 30.0, 60.0, true);
        assert!(!g.oferta, "a good stretch resets the 20 s");
        // A long hitch frame does not count as slow play.
        let mut g = Guarda::default();
        g.observa(1.0, 5.0, 60.0, true);
        assert!(!g.oferta);
    }

    /// Owner rule: at the lowest setting there is nothing to step down to,
    /// so the offer never appears, however slow it runs.
    #[test]
    fn a_guarda_fica_calada_no_nivel_mais_baixo() {
        let mut g = Guarda::default();
        joga(&mut g, 0.0, 120.0, 10.0, 60.0, false);
        assert!(!g.oferta);
        // And the gate the game passes in is "a lighter preset exists".
        assert!(!cg::ha_nivel_abaixo(&cg::preset(0)));
        for i in 1..cg::PRESETS.len() {
            assert!(cg::ha_nivel_abaixo(&cg::preset(i)), "{i}");
        }
        // Low with a 60 cap or AA is still the lowest.
        let mut baixo = cg::preset(0);
        baixo.fps = 60;
        baixo.antialias = 4;
        assert!(!cg::ha_nivel_abaixo(&baixo));
        // Already offered, then the graphics reach the lowest: it goes away.
        let mut g = Guarda::default();
        joga(&mut g, 0.0, 25.0, 10.0, 60.0, true);
        assert!(g.oferta);
        g.observa(26.0, 0.1, 60.0, false);
        assert!(!g.oferta);
    }
}
