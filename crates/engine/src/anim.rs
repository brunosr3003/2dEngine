//! Sistema de animacao de sprites por frames.
//!
//! Fluxo tipico:
//! 1. Criar clips no `AnimRegistry` (uma vez, no init).
//! 2. Cada entidade animavel tem um `AnimPlayer` como componente hecs.
//! 3. Chamar `AnimPlayer::update(dt)` em todo frame de simulacao.
//! 4. Usar `AnimPlayer::uvs()` para preencher o Sprite antes de push no batch.

use glam::Vec2;

/// Um frame de animacao: onde no atlas ele esta e quanto tempo dura.
#[derive(Debug, Clone, Copy)]
pub struct AnimFrame {
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// Duracao deste frame em segundos.
    pub duration: f32,
}

impl AnimFrame {
    pub fn uniform(uv_min: Vec2, uv_max: Vec2, fps: f32) -> Self {
        Self { uv_min, uv_max, duration: 1.0 / fps.max(0.001) }
    }
}

/// Sequencia de frames. Registrada no `AnimRegistry` com um nome.
#[derive(Debug, Clone)]
pub struct AnimClip {
    pub frames: Vec<AnimFrame>,
    pub looping: bool,
}

impl AnimClip {
    pub fn new(frames: Vec<AnimFrame>, looping: bool) -> Self {
        Self { frames, looping }
    }

    /// Atalho: todos os frames tem a mesma duracao (fps uniforme).
    pub fn from_row(
        atlas_w: u32,
        atlas_h: u32,
        row_y: u32,
        frame_w: u32,
        frame_h: u32,
        count: u32,
        fps: f32,
        looping: bool,
    ) -> Self {
        let iw = atlas_w as f32;
        let ih = atlas_h as f32;
        let dur = 1.0 / fps.max(0.001);
        let frames = (0..count)
            .map(|i| AnimFrame {
                uv_min: Vec2::new(
                    (i * frame_w) as f32 / iw,
                    row_y as f32 / ih,
                ),
                uv_max: Vec2::new(
                    ((i + 1) * frame_w) as f32 / iw,
                    (row_y + frame_h) as f32 / ih,
                ),
                duration: dur,
            })
            .collect();
        Self { frames, looping }
    }
}

/// Registro central de clips. Criado uma vez no init do jogo e passado
/// por referencia imutavel para os `AnimPlayer::update`.
#[derive(Default)]
pub struct AnimRegistry {
    clips: Vec<AnimClip>,
    names: std::collections::HashMap<String, usize>,
}

impl AnimRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, name: impl Into<String>, clip: AnimClip) -> usize {
        let id = self.clips.len();
        self.names.insert(name.into(), id);
        self.clips.push(clip);
        id
    }

    pub fn get(&self, id: usize) -> Option<&AnimClip> { self.clips.get(id) }

    pub fn id_of(&self, name: &str) -> Option<usize> { self.names.get(name).copied() }
}

/// Componente ECS por entidade. Controla qual clip esta tocando e em qual
/// frame. Atualizar com `update(dt, registry)`, ler UVs com `uvs()`.
#[derive(Debug, Clone)]
pub struct AnimPlayer {
    pub clip_id: usize,
    pub frame: usize,
    pub timer: f32,
    /// true quando clip nao-looping terminou.
    pub done: bool,
    /// Espelha o sprite horizontalmente (caminha para esquerda).
    pub flip_x: bool,
}

impl AnimPlayer {
    pub fn new(clip_id: usize) -> Self {
        Self { clip_id, frame: 0, timer: 0.0, done: false, flip_x: false }
    }

    /// Troca de clip e reseta se nao era o mesmo.
    pub fn set_clip(&mut self, id: usize) {
        if self.clip_id != id {
            self.clip_id = id;
            self.frame = 0;
            self.timer = 0.0;
            self.done = false;
        }
    }

    /// Avanca a animacao. Chamar uma vez por tick de simulacao (dt fixo).
    pub fn update(&mut self, dt: f32, registry: &AnimRegistry) {
        let Some(clip) = registry.get(self.clip_id) else { return };
        if clip.frames.is_empty() || self.done { return }

        self.timer += dt;
        while self.timer >= clip.frames[self.frame].duration {
            self.timer -= clip.frames[self.frame].duration;
            if self.frame + 1 < clip.frames.len() {
                self.frame += 1;
            } else if clip.looping {
                self.frame = 0;
            } else {
                self.done = true;
                return;
            }
        }
    }

    /// UVs do frame atual. Se `flip_x`, inverte UV horizontalmente.
    pub fn uvs(&self, registry: &AnimRegistry) -> (Vec2, Vec2) {
        let fallback = (Vec2::ZERO, Vec2::ONE);
        let Some(clip) = registry.get(self.clip_id) else { return fallback };
        let Some(frame) = clip.frames.get(self.frame) else { return fallback };
        if self.flip_x {
            (Vec2::new(frame.uv_max.x, frame.uv_min.y), Vec2::new(frame.uv_min.x, frame.uv_max.y))
        } else {
            (frame.uv_min, frame.uv_max)
        }
    }
}
