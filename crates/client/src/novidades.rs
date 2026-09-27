//! Notas incluídas no próprio aplicativo, disponíveis mesmo sem conexão.
use crate::{hud_estilo as estilo, ui};
use macroquad::prelude::*;

const NOTAS: &str = include_str!("../../../docs/PATCHNOTES.txt");

pub struct Novidades {
    aberta: bool,
    scroll: f32,
    arrasto: Option<f32>,
}

fn arquivo_lido() -> Option<std::path::PathBuf> {
    crate::lembranca::caminho().map(|p| p.with_extension("patchnotes"))
}

impl Default for Novidades {
    fn default() -> Self {
        let lidas = arquivo_lido().and_then(|p| std::fs::read_to_string(p).ok());
        Self {
            aberta: lidas.as_deref() != Some(NOTAS),
            scroll: 0.0,
            arrasto: None,
        }
    }
}

impl Novidades {
    /// True captura esta tela: campos de login não recebem o mesmo clique.
    pub fn desenha(&mut self) -> bool {
        if !self.aberta {
            if !ui::botao(Rect::new(16.0, 16.0, 180.0, 36.0), "Novidades", true) {
                return false;
            }
            self.aberta = true;
            self.scroll = 0.0;
        }
        let w = (screen_width() - 32.0).clamp(240.0, 600.0);
        let h = (screen_height() - 32.0).clamp(240.0, 590.0);
        let r = Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        );
        estilo::painel_destaque(r, ui::OURO);
        estilo::texto(r.x + 24.0, r.y + 34.0, "NOVIDADES DO TEMPEST", 20, ui::OURO);
        let mut notas = NOTAS.lines();
        estilo::texto(
            r.x + 24.0,
            r.y + 59.0,
            notas.next().unwrap_or(""),
            13,
            ui::APOIO,
        );
        let corpo = Rect::new(r.x + 24.0, r.y + 82.0, r.w - 48.0, r.h - 154.0);
        let linhas = quebra(&notas.collect::<Vec<_>>().join("\n"), corpo.w, &|s| {
            estilo::medir(s, 15)
        });
        let max = (linhas.len() as f32 * 25.0 - corpo.h).max(0.0);
        let mouse = Vec2::from(mouse_position());
        if corpo.contains(mouse) {
            self.scroll -= mouse_wheel().1 * 28.0;
            if is_mouse_button_pressed(MouseButton::Left) {
                self.arrasto = Some(mouse.y);
            }
        }
        if is_mouse_button_down(MouseButton::Left) {
            if let Some(anterior) = self.arrasto {
                self.scroll += anterior - mouse.y;
                self.arrasto = Some(mouse.y);
            }
        } else {
            self.arrasto = None;
        }
        self.scroll = self.scroll.clamp(0.0, max);
        for (i, linha) in linhas.iter().enumerate() {
            let y = corpo.y + 18.0 + i as f32 * 25.0 - self.scroll;
            if y >= corpo.y + 15.0 && y <= corpo.bottom() {
                estilo::texto(corpo.x, y, linha, 15, estilo::TEXTO);
            }
        }
        if max > 0.0 {
            if ui::botao(
                Rect::new(r.x + 24.0, r.bottom() - 56.0, 44.0, 34.0),
                "↑",
                true,
            ) {
                self.scroll = (self.scroll - 100.0).max(0.0);
            }
            if ui::botao(
                Rect::new(r.x + 76.0, r.bottom() - 56.0, 44.0, 34.0),
                "↓",
                true,
            ) {
                self.scroll = (self.scroll + 100.0).min(max);
            }
        }
        if ui::botao(
            Rect::new(r.right() - 166.0, r.bottom() - 56.0, 142.0, 34.0),
            "Continuar",
            true,
        ) || is_key_pressed(KeyCode::Escape)
        {
            self.aberta = false;
            self.arrasto = None;
            if let Some(p) = arquivo_lido() {
                let _ = std::fs::write(p, NOTAS);
            }
        }
        true
    }
}

fn quebra(texto: &str, largura: f32, medir: &impl Fn(&str) -> f32) -> Vec<String> {
    let mut linhas = Vec::new();
    for paragrafo in texto.lines() {
        let mut linha = String::new();
        for palavra in paragrafo.split_whitespace() {
            let candidata = if linha.is_empty() {
                palavra.to_string()
            } else {
                format!("{linha} {palavra}")
            };
            if !linha.is_empty() && medir(&candidata) > largura {
                linhas.push(std::mem::take(&mut linha));
            }
            if !linha.is_empty() {
                linha.push(' ');
            }
            linha.push_str(palavra);
        }
        linhas.push(linha);
    }
    linhas
}

pub async fn previa() {
    let mut notas = Novidades {
        aberta: true,
        scroll: 0.0,
        arrasto: None,
    };
    for _ in 0..5 {
        ui::fundo();
        notas.desenha();
        next_frame().await;
    }
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-patchnotes.png".into());
    ui::fundo();
    notas.desenha();
    unsafe { get_internal_gl().flush(); }
    get_screen_data().export_png(&saida);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quebra_preserva_paragrafos_e_texto() {
        let linhas = quebra("um dois três\n\nfim", 8.0, &|s| s.chars().count() as f32);
        assert_eq!(linhas, ["um dois", "três", "", "fim"]);
    }
}
