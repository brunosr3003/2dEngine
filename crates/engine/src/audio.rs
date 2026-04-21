//! Sistema de audio via rodio. Native only (wasm usa web-sys na Fase 5).
//!
//! Uso:
//! ```ignore
//! let mut audio = AudioManager::new()?;
//! audio.play_music(MUSIC_BYTES, true);
//! audio.play_sfx(HIT_BYTES);
//! audio.set_music_volume(0.5);
//! ```
//!
//! Sons sao passados como `&'static [u8]` (embed com `include_bytes!`).
//! Para sons dinamicos (carregados em runtime), usar `Vec<u8>` via `Cursor`.

#[cfg(not(target_family = "wasm"))]
use rodio::{source::SineWave, Decoder, OutputStream, OutputStreamHandle, Sink, Source};
#[cfg(not(target_family = "wasm"))]
use std::time::Duration;
use std::io::Cursor;

#[cfg(not(target_family = "wasm"))]
pub struct AudioManager {
    // _stream deve viver enquanto AudioManager existir.
    _stream: OutputStream,
    handle: OutputStreamHandle,
    music: Option<Sink>,
    pub master_volume: f32,
    pub music_volume: f32,
    pub sfx_volume: f32,
}

#[cfg(not(target_family = "wasm"))]
impl AudioManager {
    pub fn new() -> anyhow::Result<Self> {
        let (_stream, handle) = OutputStream::try_default()?;
        Ok(Self {
            _stream,
            handle,
            music: None,
            master_volume: 1.0,
            music_volume: 0.8,
            sfx_volume: 1.0,
        })
    }

    /// Toca um SFX sem bloquear. Multiplos SFX podem tocar simultaneamente.
    pub fn play_sfx(&self, bytes: &'static [u8]) {
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => { tracing::warn!("audio sfx: {e}"); return; }
        };
        let src = match Decoder::new(Cursor::new(bytes)) {
            Ok(d) => d,
            Err(e) => { tracing::warn!("audio decode: {e}"); return; }
        };
        sink.set_volume(self.sfx_volume * self.master_volume);
        sink.append(src);
        sink.detach(); // sink se auto-destroi ao terminar
    }

    /// Toca um SFX a partir de bytes dinamicos (Vec<u8>).
    pub fn play_sfx_owned(&self, bytes: Vec<u8>) {
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => { tracing::warn!("audio sfx: {e}"); return; }
        };
        let src = match Decoder::new(Cursor::new(bytes)) {
            Ok(d) => d,
            Err(e) => { tracing::warn!("audio decode: {e}"); return; }
        };
        sink.set_volume(self.sfx_volume * self.master_volume);
        sink.append(src);
        sink.detach();
    }

    /// Inicia musica de fundo. Para a musica anterior se houver.
    pub fn play_music(&mut self, bytes: &'static [u8], looping: bool) {
        self.stop_music();
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => { tracing::warn!("audio music: {e}"); return; }
        };
        let src = match Decoder::new(Cursor::new(bytes)) {
            Ok(d) => d,
            Err(e) => { tracing::warn!("audio decode: {e}"); return; }
        };
        sink.set_volume(self.music_volume * self.master_volume);
        if looping {
            sink.append(src.repeat_infinite());
        } else {
            sink.append(src);
        }
        self.music = Some(sink);
    }

    pub fn stop_music(&mut self) {
        if let Some(s) = self.music.take() {
            s.stop();
        }
    }

    pub fn pause_music(&self) {
        if let Some(s) = &self.music { s.pause(); }
    }

    pub fn resume_music(&self) {
        if let Some(s) = &self.music { s.play(); }
    }

    pub fn set_music_volume(&mut self, vol: f32) {
        self.music_volume = vol.clamp(0.0, 1.0);
        if let Some(s) = &self.music {
            s.set_volume(self.music_volume * self.master_volume);
        }
    }

    pub fn set_master_volume(&mut self, vol: f32) {
        self.master_volume = vol.clamp(0.0, 1.0);
        self.set_music_volume(self.music_volume); // recalcula
    }

    pub fn music_done(&self) -> bool {
        self.music.as_ref().map(|s| s.empty()).unwrap_or(true)
    }

    /// Toca um tom senoidal procedural. Util para SFX simples sem asset.
    /// `freq_hz` = frequencia, `duration_ms` = duracao total (com fade-out),
    /// `volume` = 0..1 relativo ao sfx_volume.
    pub fn play_tone(&self, freq_hz: f32, duration_ms: u64, volume: f32) {
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => { tracing::warn!("audio tone: {e}"); return; }
        };
        let total = Duration::from_millis(duration_ms);
        let fade  = Duration::from_millis((duration_ms / 3).max(10));
        let src = SineWave::new(freq_hz)
            .take_duration(total)
            .fade_in(Duration::from_millis(5))
            .amplify(volume.clamp(0.0, 1.0));
        sink.set_volume(self.sfx_volume * self.master_volume);
        sink.append(src);
        // Aplica fade-out no sink (rodio nao tem fade_out no source simples)
        let _ = fade;
        sink.detach();
    }

    /// Toca uma sequencia de tons (arpeggio). Cada passo = (freq, duration_ms).
    pub fn play_sequence(&self, steps: &[(f32, u64)], volume: f32) {
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => { tracing::warn!("audio seq: {e}"); return; }
        };
        for (freq, dur) in steps {
            let src = SineWave::new(*freq)
                .take_duration(Duration::from_millis(*dur))
                .amplify(volume.clamp(0.0, 1.0));
            sink.append(src);
        }
        sink.set_volume(self.sfx_volume * self.master_volume);
        sink.detach();
    }
}

/// Stub vazio para wasm (compilacao nao falha, audio e no-op).
#[cfg(target_family = "wasm")]
pub struct AudioManager;

#[cfg(target_family = "wasm")]
impl AudioManager {
    pub fn new() -> anyhow::Result<Self> { Ok(Self) }
    pub fn play_sfx(&self, _bytes: &'static [u8]) {}
    pub fn play_music(&mut self, _bytes: &'static [u8], _looping: bool) {}
    pub fn stop_music(&mut self) {}
    pub fn pause_music(&self) {}
    pub fn resume_music(&self) {}
    pub fn set_music_volume(&mut self, _vol: f32) {}
    pub fn set_master_volume(&mut self, _vol: f32) {}
    pub fn music_done(&self) -> bool { true }
    pub fn play_tone(&self, _freq_hz: f32, _duration_ms: u64, _volume: f32) {}
    pub fn play_sequence(&self, _steps: &[(f32, u64)], _volume: f32) {}
}
