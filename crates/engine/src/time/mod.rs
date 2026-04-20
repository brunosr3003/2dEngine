//! Fixed timestep — desacopla a simulacao do framerate de render.
//!
//! Uso tipico:
//! ```ignore
//! let mut ts = FixedTimestep::new(30);
//! loop {
//!     let ticks = ts.advance();
//!     for _ in 0..ticks { world.step(ts.step().as_secs_f32()); }
//!     render(world, ts.alpha());  // alpha = interpolacao para frames entre ticks
//! }
//! ```

use std::time::{Duration, Instant};

pub struct FixedTimestep {
    step: Duration,
    accumulator: Duration,
    last: Instant,
    /// Limite para evitar spiral-of-death (muitos ticks apos um freeze).
    max_lag: Duration,
}

impl FixedTimestep {
    pub fn new(hz: u32) -> Self {
        let step = Duration::from_secs_f64(1.0 / hz.max(1) as f64);
        Self {
            step,
            accumulator: Duration::ZERO,
            last: Instant::now(),
            max_lag: step * 5,
        }
    }

    /// Avanca o relogio e retorna quantos ticks a simulacao deve rodar.
    pub fn advance(&mut self) -> u32 {
        let now = Instant::now();
        let mut dt = now.saturating_duration_since(self.last);
        self.last = now;
        if dt > self.max_lag { dt = self.max_lag; }
        self.accumulator += dt;
        let mut ticks = 0;
        while self.accumulator >= self.step {
            self.accumulator -= self.step;
            ticks += 1;
        }
        ticks
    }

    /// Fracao do proximo tick acumulada — use para interpolar no render.
    pub fn alpha(&self) -> f32 {
        (self.accumulator.as_secs_f64() / self.step.as_secs_f64()) as f32
    }

    pub fn step(&self) -> Duration { self.step }
}
