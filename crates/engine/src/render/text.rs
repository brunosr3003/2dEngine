//! Renderizacao de texto via bitmap font no SpriteBatch.
//!
//! Usa o mesmo atlas de sprites, sem pipeline separado. Ideal para pixel art.
//!
//! Formato esperado do atlas de fonte: grade de caracteres ASCII 32–126,
//! dispostos em linhas de `cols` caracteres, iniciando na posicao
//! (origin_x, origin_y) dentro do atlas.
//!
//! Exemplo: fonte 8x8, 16 colunas, 6 linhas (chars 32..127)
//! Caracter 'A' (ASCII 65) -> index 65-32 = 33 -> col=1, row=2

use super::{Sprite, SpriteBatch};
use glam::{Vec2, Vec4};

/// Fonte bitmap. Criada uma vez no init do jogo.
pub struct BitmapFont {
    /// Tamanho do caracter em tiles de mundo (usa o mesmo zoom da camera).
    pub char_size: Vec2,
    /// Espacamento horizontal entre caracteres (pode ser <= char_size.x).
    pub advance_x: f32,
    /// Espacamento vertical entre linhas.
    pub line_height: f32,
    /// UVs [0..96] para ASCII 32..=127. None = caracter nao disponivel.
    uvs: [Option<(Vec2, Vec2)>; 96],
}

impl BitmapFont {
    /// Cria uma fonte a partir de uma grade uniforme de caracteres no atlas.
    ///
    /// * `atlas_w/h`   — dimensoes do atlas em pixels.
    /// * `origin_x/y`  — pixel de inicio da grade de caracteres no atlas.
    /// * `char_px_w/h` — tamanho de cada caractere em pixels no atlas.
    /// * `cols`        — caracteres por linha na grade.
    /// * `char_size`   — tamanho em unidades de mundo para renderizar.
    pub fn from_grid(
        atlas_w: u32,
        atlas_h: u32,
        origin_x: u32,
        origin_y: u32,
        char_px_w: u32,
        char_px_h: u32,
        cols: u32,
        char_size: Vec2,
    ) -> Self {
        let iw = atlas_w as f32;
        let ih = atlas_h as f32;
        let mut uvs = [None; 96];

        for i in 0usize..96 {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;
            let px = origin_x + col * char_px_w;
            let py = origin_y + row * char_px_h;
            uvs[i] = Some((
                Vec2::new(px as f32 / iw, py as f32 / ih),
                Vec2::new((px + char_px_w) as f32 / iw, (py + char_px_h) as f32 / ih),
            ));
        }

        let advance_x = char_size.x;
        let line_height = char_size.y * 1.2;
        Self { char_size, advance_x, line_height, uvs }
    }

    /// Renderiza `text` a partir de `pos` (canto superior-esquerdo em mundo).
    /// Suporta '\n' para quebra de linha.
    /// `scale` multiplica `char_size` — use 1.0 para tamanho padrao.
    pub fn draw(
        &self,
        text: &str,
        pos: Vec2,
        scale: f32,
        color: Vec4,
        batch: &mut SpriteBatch,
    ) {
        self.draw_depth(text, pos, scale, color, 0.0, batch);
    }

    /// Como `draw`, mas permite especificar a profundidade (z-order) dos
    /// caracteres. Use `layer::HUD` ou similar para garantir que texto fique
    /// acima de entidades.
    pub fn draw_depth(
        &self,
        text: &str,
        pos: Vec2,
        scale: f32,
        color: Vec4,
        depth: f32,
        batch: &mut SpriteBatch,
    ) {
        let size = self.char_size * scale;
        let adv  = self.advance_x * scale;
        let lh   = self.line_height * scale;
        let mut cursor = pos;

        for ch in text.chars() {
            if ch == '\n' {
                cursor.x = pos.x;
                cursor.y -= lh;
                continue;
            }
            let idx = ch as usize;
            if idx < 32 || idx > 127 {
                cursor.x += adv;
                continue;
            }
            if let Some((uv_min, uv_max)) = self.uvs[idx - 32] {
                // Centro do sprite = cursor + meio do char
                let center = cursor + Vec2::new(size.x * 0.5, -size.y * 0.5);
                batch.push(&Sprite {
                    position: center,
                    size,
                    uv_min,
                    uv_max,
                    tint: color,
                    depth,
                    ..Default::default()
                });
            }
            cursor.x += adv;
        }
    }

    /// Largura total de `text` em unidades de mundo (sem scale).
    pub fn measure_width(&self, text: &str) -> f32 {
        let mut max = 0.0f32;
        let mut cur = 0.0f32;
        for ch in text.chars() {
            if ch == '\n' {
                max = max.max(cur);
                cur = 0.0;
            } else {
                cur += self.advance_x;
            }
        }
        max.max(cur)
    }
}
