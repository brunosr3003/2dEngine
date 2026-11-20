//! The clock the drawing reads.
//!
//! In play it is macroquad's own (`get_time`, `get_frame_time`), untouched.
//! The ad capture (`filme`, debug desktop only) pins it to a fixed 1/60 s
//! step per frame, so waves, auras and animations move exactly as fast in
//! the video however long each frame took to render and encode. Files that
//! draw the world import these two names over the prelude's.

use std::cell::Cell;

thread_local! {
    /// (time, frame time) while a capture holds the clock.
    static FIXO: Cell<Option<(f64, f32)>> = const { Cell::new(None) };
}

pub fn get_time() -> f64 {
    FIXO.with(|f| f.get())
        .map_or_else(macroquad::time::get_time, |(t, _)| t)
}

pub fn get_frame_time() -> f32 {
    FIXO.with(|f| f.get())
        .map_or_else(macroquad::time::get_frame_time, |(_, dt)| dt)
}

/// Holds (or with `None` releases) the clock. Only the capture calls it.
#[allow(dead_code)]
pub fn fixa(t: Option<(f64, f32)>) {
    FIXO.with(|f| f.set(t));
}

pub fn fixo() -> bool {
    FIXO.with(|f| f.get()).is_some()
}

#[cfg(test)]
mod testes {
    #[test]
    fn so_a_captura_segura_o_relogio() {
        assert!(!super::fixo(), "the game's clock is free by default");
        super::fixa(Some((12.5, 1.0 / 60.0)));
        assert_eq!(super::get_time(), 12.5);
        assert_eq!(super::get_frame_time(), 1.0 / 60.0);
        super::fixa(None);
        assert!(!super::fixo());
    }
}
