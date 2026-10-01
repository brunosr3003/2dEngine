//! Menu → System → Graphics: how much of the world is drawn and how.
//!
//! Every choice except anti-aliasing applies on the next frame. The panel
//! itself only stores the choice: the renderer reads it through the free
//! functions below (`raio_terreno`, `forracao`, `ondas`…), so the terrain,
//! the sea and the skill effects never need a reference to this struct.
//!
//! Saved per DEVICE, next to the remembered login (`lembranca::caminho`): the
//! same character on a phone and on a desktop wants different settings.
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU8, Ordering};

use macroquad::prelude::*;

use crate::hud_estilo as estilo;
use crate::hud_layout;

const MOBILE: bool = cfg!(any(target_os = "ios", target_os = "android"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sombras {
    Desligadas,
    Leves,
    Bonitas,
}

impl Sombras {
    pub fn valor(self) -> &'static str {
        match self {
            Self::Desligadas => "0",
            Self::Leves => "1",
            Self::Bonitas => "2",
        }
    }
}

/// MSAA sample counts on offer (1 = off).
pub const OPCOES_AA: [i32; 4] = [1, 2, 4, 8];

/// The saved anti-aliasing choice. MSAA is picked when the window is
/// created, so `window_conf` reads this and a change applies on the next launch.
pub fn antialias_salvo() -> i32 {
    let padrao = if MOBILE { 1 } else { 4 };
    crate::lembranca::caminho()
        .and_then(|p| std::fs::read_to_string(p.with_file_name("antialias.prefs")).ok())
        .and_then(|v| v.trim().parse().ok())
        .filter(|n| OPCOES_AA.contains(n))
        .unwrap_or(padrao)
}

// ── what the renderer reads ──────────────────────────────────────────────

/// Terrain radius, in chunks, around the player (the default is the radius
/// the camera band was measured against — see `render3d::PITCH_MIN`).
pub const RAIO_PADRAO: i32 = 5;
/// View distance: (chunk radius, name). A 16 u chunk each; 5 is the old
/// fixed radius. Mesh grows with the square: ~130 MB at 5, ~400 MB at 9,
/// ~680 MB at 12 — so Extreme is desktop only.
const DISTANCIAS: &[(i32, &str)] = if MOBILE {
    &[(3, "Near"), (5, "Normal"), (7, "Far"), (9, "Very far")]
} else {
    &[(3, "Near"), (5, "Normal"), (7, "Far"), (9, "Very far"), (12, "Extreme")]
};
/// Ground cover kept, in percent: (value, name). Trees are not here: a tree
/// blocks the path and is gathered, so it is gameplay, not decoration.
const FORRACOES: &[(u8, &str)] = &[(30, "Sparse"), (65, "Medium"), (100, "Full")];
/// Frame-rate cap: (fps, name); 0 = no cap beyond the display's vsync.
const LIMITES_FPS: &[(u16, &str)] = &[(30, "30"), (60, "60"), (0, "Unlimited")];

static RAIO: AtomicI32 = AtomicI32::new(RAIO_PADRAO);
static FORRACAO: AtomicU8 = AtomicU8::new(100);
static FPS: AtomicU16 = AtomicU16::new(0);
static ONDAS: AtomicBool = AtomicBool::new(true);
static EFEITOS_DOS_OUTROS: AtomicBool = AtomicBool::new(true);

/// How many chunks around the player are kept loaded.
pub fn raio_terreno() -> i32 {
    RAIO.load(Ordering::Relaxed)
}

/// The distance fog around the player for the current view distance: (where
/// it starts, where it is solid sky), in world units from the player.
///
/// The loaded square ends `(raio + 0.5)` chunks out along each axis; the fog
/// is solid half a chunk before that, so neither the square's edge nor its
/// corners ever show. It starts at 60% of the way, a haze rather than a wall.
pub fn neblina() -> (f32, f32) {
    let fim = raio_terreno() as f32 * crate::terreno::CHUNK as f32 * shared::terreno::BLOCO;
    (fim * 0.6, fim)
}

/// Percent of the ground cover (grass, bushes, flowers) that is baked.
pub fn forracao() -> u8 {
    FORRACAO.load(Ordering::Relaxed)
}

/// Whether the sea moves (Gerstner waves) or lies flat.
pub fn ondas() -> bool {
    ONDAS.load(Ordering::Relaxed)
}

/// Whether OTHER PLAYERS' skill effects are drawn. Monsters' and bosses'
/// always are: what a monster is about to do is information, not decoration.
pub fn efeitos_dos_outros() -> bool {
    EFEITOS_DOS_OUTROS.load(Ordering::Relaxed)
}

/// For `MMO_PREVIA_GRAFICOS`: render the world at a given view distance and
/// ground cover without going through the panel.
#[cfg(debug_assertions)]
pub fn define_para_previa(raio: i32, forracao: u8) {
    RAIO.store(raio, Ordering::Relaxed);
    FORRACAO.store(forracao, Ordering::Relaxed);
}

thread_local! {
    static ULTIMO_QUADRO: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Holds the frame to the chosen cap. Called after drawing, like
/// `Economia::segurar_quadro` (which wins while battery saver is on).
pub fn segurar_quadro() {
    let fps = FPS.load(Ordering::Relaxed);
    ULTIMO_QUADRO.with(|u| {
        if fps > 0 {
            let falta = 1.0 / fps as f64 - (get_time() - u.get());
            if falta > 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f64(falta.min(0.2)));
            }
        }
        u.set(get_time());
    });
}

// ── presets ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
struct Escolha {
    sombras: Sombras,
    antialias: i32,
    raio: i32,
    forracao: u8,
    fps: u16,
    ondas: bool,
    efeitos_dos_outros: bool,
}

const PRESETS: &[&str] = &["Low", "Medium", "High", "Ultra"];

/// High is what the game shipped with before this panel existed (on desktop),
/// so a player who never opens it sees no change.
fn preset(i: usize) -> Escolha {
    let maior = DISTANCIAS[DISTANCIAS.len() - 1].0;
    match i {
        0 => Escolha {
            sombras: Sombras::Desligadas,
            antialias: 1,
            raio: 3,
            forracao: 30,
            fps: 30,
            ondas: false,
            efeitos_dos_outros: false,
        },
        1 => Escolha {
            sombras: Sombras::Leves,
            antialias: if MOBILE { 1 } else { 2 },
            raio: RAIO_PADRAO,
            forracao: 65,
            fps: 60,
            ondas: true,
            efeitos_dos_outros: true,
        },
        2 => Escolha {
            sombras: Sombras::Leves,
            antialias: if MOBILE { 2 } else { 4 },
            raio: RAIO_PADRAO,
            forracao: 100,
            fps: 0,
            ondas: true,
            efeitos_dos_outros: true,
        },
        _ => Escolha {
            sombras: Sombras::Bonitas,
            antialias: if MOBILE { 4 } else { 8 },
            raio: maior,
            forracao: 100,
            fps: 0,
            ondas: true,
            efeitos_dos_outros: true,
        },
    }
}

// ── saving ───────────────────────────────────────────────────────────────

fn arquivo() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_file_name("graficos.prefs"))
}

/// `key=value` lines. An unknown key or value is ignored and keeps the
/// default, so an old or hand-edited file never breaks the game.
fn le(texto: &str, e: &mut Escolha) {
    for linha in texto.lines() {
        let Some((k, v)) = linha.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "distancia" => {
                if let Some(&(r, _)) = v.parse().ok().and_then(|n: i32| DISTANCIAS.iter().find(|d| d.0 == n)) {
                    e.raio = r;
                }
            }
            "forracao" => {
                if let Some(&(p, _)) = v.parse().ok().and_then(|n: u8| FORRACOES.iter().find(|d| d.0 == n)) {
                    e.forracao = p;
                }
            }
            "fps" => {
                if let Some(&(f, _)) = v.parse().ok().and_then(|n: u16| LIMITES_FPS.iter().find(|d| d.0 == n)) {
                    e.fps = f;
                }
            }
            "ondas" => e.ondas = v != "0",
            "efeitos" => e.efeitos_dos_outros = v != "0",
            _ => {}
        }
    }
}

fn escreve(e: &Escolha) -> String {
    format!(
        "distancia={}\nforracao={}\nfps={}\nondas={}\nefeitos={}\n",
        e.raio, e.forracao, e.fps, e.ondas as u8, e.efeitos_dos_outros as u8
    )
}

// ── the panel ────────────────────────────────────────────────────────────

pub struct ConfigGraficos {
    pub aberto: bool,
    pub sombras: Sombras,
    pub antialias: i32,
    /// What the window was opened with; differs from `antialias` until restart.
    antialias_no_inicio: i32,
    raio: i32,
    forracao: u8,
    fps: u16,
    ondas: bool,
    efeitos_dos_outros: bool,
    rolagem: crate::rolagem::Rolagem,
}

impl Default for ConfigGraficos {
    fn default() -> Self {
        let sombras = crate::lembranca::caminho()
            .and_then(|p| std::fs::read_to_string(p.with_file_name("sombras.prefs")).ok())
            .map_or(Sombras::Leves, |v| match v.trim() {
                "0" => Sombras::Desligadas,
                "2" => Sombras::Bonitas,
                _ => Sombras::Leves,
            });
        let antialias = antialias_salvo();
        let mut e = preset(2);
        e.sombras = sombras;
        e.antialias = antialias;
        if let Some(texto) = arquivo().and_then(|p| std::fs::read_to_string(p).ok()) {
            le(&texto, &mut e);
        }
        let c = Self {
            aberto: false,
            sombras,
            antialias,
            antialias_no_inicio: antialias,
            raio: e.raio,
            forracao: e.forracao,
            fps: e.fps,
            ondas: e.ondas,
            efeitos_dos_outros: e.efeitos_dos_outros,
            rolagem: Default::default(),
        };
        c.publica();
        c
    }
}

/// One row of exclusive choices.
struct Linha {
    titulo: &'static str,
    opcoes: Vec<&'static str>,
    marcada: Option<usize>,
    dica: Option<&'static str>,
}

impl Linha {
    fn altura(&self) -> f32 {
        if self.dica.is_some() { 100.0 } else { 80.0 }
    }
}

impl ConfigGraficos {
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.rolagem.zera();
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    fn escolha(&self) -> Escolha {
        Escolha {
            sombras: self.sombras,
            antialias: self.antialias,
            raio: self.raio,
            forracao: self.forracao,
            fps: self.fps,
            ondas: self.ondas,
            efeitos_dos_outros: self.efeitos_dos_outros,
        }
    }

    fn aplica(&mut self, e: Escolha) {
        let antes = self.escolha();
        self.sombras = e.sombras;
        self.antialias = e.antialias;
        self.raio = e.raio;
        self.forracao = e.forracao;
        self.fps = e.fps;
        self.ondas = e.ondas;
        self.efeitos_dos_outros = e.efeitos_dos_outros;
        self.publica();
        let Some(p) = crate::lembranca::caminho() else { return };
        if antes.sombras != e.sombras {
            let _ = std::fs::write(p.with_file_name("sombras.prefs"), e.sombras.valor());
        }
        if antes.antialias != e.antialias {
            let _ = std::fs::write(p.with_file_name("antialias.prefs"), e.antialias.to_string());
        }
        let _ = std::fs::write(p.with_file_name("graficos.prefs"), escreve(&e));
    }

    /// Hands the choice to the renderer.
    fn publica(&self) {
        RAIO.store(self.raio, Ordering::Relaxed);
        FORRACAO.store(self.forracao, Ordering::Relaxed);
        FPS.store(self.fps, Ordering::Relaxed);
        ONDAS.store(self.ondas, Ordering::Relaxed);
        EFEITOS_DOS_OUTROS.store(self.efeitos_dos_outros, Ordering::Relaxed);
    }

    fn linhas(&self) -> Vec<Linha> {
        let e = self.escolha();
        vec![
            Linha {
                titulo: "Preset",
                opcoes: PRESETS.to_vec(),
                marcada: (0..PRESETS.len()).find(|&i| preset(i) == e),
                dica: None,
            },
            Linha {
                titulo: "View distance",
                opcoes: DISTANCIAS.iter().map(|d| d.1).collect(),
                marcada: DISTANCIAS.iter().position(|d| d.0 == self.raio),
                dica: Some("Fog hides the edge. Farther lets the camera tilt lower."),
            },
            Linha {
                titulo: "Shadows",
                opcoes: vec!["Off", "Light", "Pretty"],
                marcada: Some(match self.sombras {
                    Sombras::Desligadas => 0,
                    Sombras::Leves => 1,
                    Sombras::Bonitas => 2,
                }),
                dica: Some("Pretty: late-afternoon light and scenery shadows."),
            },
            Linha {
                titulo: "Grass and bushes",
                opcoes: FORRACOES.iter().map(|d| d.1).collect(),
                marcada: FORRACOES.iter().position(|d| d.0 == self.forracao),
                dica: Some("Trees, rocks and anything solid always show."),
            },
            Linha {
                titulo: "Frame-rate cap",
                opcoes: LIMITES_FPS.iter().map(|d| d.1).collect(),
                marcada: LIMITES_FPS.iter().position(|d| d.0 == self.fps),
                dica: Some("30 saves battery on phones."),
            },
            Linha {
                titulo: "Sea",
                opcoes: vec!["Calm", "Waves"],
                marcada: Some(self.ondas as usize),
                dica: None,
            },
            Linha {
                titulo: "Other players' skill effects",
                opcoes: vec!["Hide", "Show"],
                marcada: Some(self.efeitos_dos_outros as usize),
                dica: Some("Monster and boss attacks always show."),
            },
            Linha {
                titulo: if self.antialias == self.antialias_no_inicio {
                    "Anti-aliasing"
                } else {
                    "Anti-aliasing (restart to apply)"
                },
                opcoes: vec!["No AA", "2x", "4x", "8x"],
                marcada: OPCOES_AA.iter().position(|&n| n == self.antialias),
                dica: None,
            },
        ]
    }

    /// Option `o` of row `l` was picked.
    fn escolhe(&mut self, l: usize, o: usize) {
        let mut e = self.escolha();
        match l {
            0 => e = preset(o),
            1 => e.raio = DISTANCIAS[o].0,
            2 => e.sombras = [Sombras::Desligadas, Sombras::Leves, Sombras::Bonitas][o],
            3 => e.forracao = FORRACOES[o].0,
            4 => e.fps = LIMITES_FPS[o].0,
            5 => e.ondas = o == 1,
            6 => e.efeitos_dos_outros = o == 1,
            _ => e.antialias = OPCOES_AA[o],
        }
        if e != self.escolha() {
            self.aplica(e);
        }
    }

    /// For the HUD preview: the content scrolled all the way down (the draw
    /// clamps it to the real end).
    #[cfg(debug_assertions)]
    pub fn rolar_ao_fim(&mut self) {
        self.rolagem.pos = f32::MAX / 4.0;
    }

    /// Draws the panel and handles the tap. Everything it changes is read by
    /// the renderer on its own, so there is nothing to hand back.
    pub fn desenha(&mut self) {
        let f = estilo::fator_texto();
        let seguro = hud_layout::tela_segura();
        let linhas = self.linhas();
        // Wide and short (a phone held sideways): two columns, so the whole
        // list fits without scrolling far.
        let duas = seguro.w >= 900.0 * f && seguro.h < 760.0 * f;
        let colunas = if duas { 2 } else { 1 };
        let por_coluna = linhas.len().div_ceil(colunas);
        let alturas: Vec<f32> = linhas
            .chunks(por_coluna)
            .map(|c| c.iter().map(|l| l.altura()).sum::<f32>())
            .collect();
        let topo = 52.0;
        let conteudo_h = (topo + alturas.iter().cloned().fold(0.0, f32::max) + 14.0) * f;
        let largura_coluna = 420.0 * f;
        let w = (largura_coluna * colunas as f32).min(seguro.w - 16.0);
        let h = conteudo_h.min(seguro.h - 16.0);
        let r = Rect::new(seguro.center().x - w * 0.5, seguro.center().y - h * 0.5, w, h);
        estilo::painel(r);
        estilo::texto(r.x + 18.0 * f, r.y + 34.0 * f, "Graphics", 20, estilo::OURO);
        let fechar = Rect::new(r.x + r.w - 44.0 * f, r.y + 8.0 * f, 36.0 * f, 36.0 * f);
        estilo::texto_centro(fechar.center().x, fechar.center().y + 7.0 * f, "X", 18, estilo::TEXTO);
        let m = Vec2::from(mouse_position());
        if crate::foco::clique() && fechar.contains(m) {
            self.fechar();
            return;
        }
        // The content scrolls under the fixed title, like the Interface panel:
        // a tap counts when the finger lifts without dragging.
        let area = Rect::new(r.x, r.y + topo * f, r.w, r.h - topo * f - 4.0 * f);
        let total = conteudo_h - topo * f;
        let toque = self.rolagem.quadro(area, total, 60.0 * f);
        let em = |b: Rect| toque.is_some_and(|p| b.contains(p) && area.contains(p));
        crate::rolagem::recortar(Some(area));
        let cw = r.w / colunas as f32;
        let mut clicado = None;
        for (c, coluna) in linhas.chunks(por_coluna).enumerate() {
            let x = r.x + c as f32 * cw + 18.0 * f;
            let largura = cw - 36.0 * f;
            let mut y = area.y - self.rolagem.pos + 18.0 * f;
            for (k, l) in coluna.iter().enumerate() {
                let indice = c * por_coluna + k;
                estilo::texto(x, y, l.titulo, 14, estilo::SUAVE);
                let n = l.opcoes.len() as f32;
                let vao = 6.0 * f;
                let bw = (largura - vao * (n - 1.0)) / n;
                for (o, nome) in l.opcoes.iter().enumerate() {
                    let b = Rect::new(x + o as f32 * (bw + vao), y + 10.0 * f, bw, 42.0 * f);
                    let marcado = l.marcada == Some(o);
                    estilo::cartao(b, b.contains(m), marcado);
                    estilo::texto_centro(
                        b.center().x,
                        b.center().y + 6.0 * f,
                        nome,
                        14,
                        if marcado { estilo::OURO } else { estilo::TEXTO },
                    );
                    if em(b) {
                        clicado = Some((indice, o));
                    }
                }
                if let Some(dica) = l.dica {
                    estilo::texto(x, y + 70.0 * f, dica, 11, estilo::SUAVE);
                }
                y += l.altura() * f;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some((l, o)) = clicado {
            self.escolhe(l, o);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are the High preset: someone who never opens the panel
    /// keeps the game exactly as it was.
    #[test]
    fn o_padrao_e_o_jogo_de_antes() {
        let e = preset(2);
        assert_eq!(e.raio, RAIO_PADRAO);
        assert_eq!(e.forracao, 100);
        assert_eq!(e.fps, 0);
        assert!(e.ondas && e.efeitos_dos_outros);
        assert_eq!(e.sombras, Sombras::Leves);
    }

    #[test]
    fn todo_preset_cabe_nas_opcoes() {
        for i in 0..PRESETS.len() {
            let e = preset(i);
            assert!(DISTANCIAS.iter().any(|d| d.0 == e.raio), "{i}: raio {}", e.raio);
            assert!(FORRACOES.iter().any(|d| d.0 == e.forracao), "{i}");
            assert!(LIMITES_FPS.iter().any(|d| d.0 == e.fps), "{i}");
            assert!(OPCOES_AA.contains(&e.antialias), "{i}");
        }
    }

    #[test]
    fn o_arquivo_vai_e_volta() {
        let mut e = preset(0);
        e.efeitos_dos_outros = true;
        let mut lido = preset(2);
        le(&escreve(&e), &mut lido);
        assert_eq!((lido.raio, lido.forracao, lido.fps, lido.ondas, lido.efeitos_dos_outros),
                   (e.raio, e.forracao, e.fps, e.ondas, e.efeitos_dos_outros));
    }

    /// A value the build does not offer (Very far saved on desktop, file
    /// copied to a phone) keeps the default instead of loading 361 chunks.
    #[test]
    fn valor_fora_da_lista_fica_no_padrao() {
        let mut e = preset(2);
        le("distancia=40\nforracao=7\nfps=144\nlixo\n", &mut e);
        assert_eq!(e, preset(2));
    }
}
