//! Interpolacao de entidades remotas entre snapshots.
//!
//! Problema: snapshots chegam a 30Hz mas renderizamos a 60/144 Hz. Sem
//! interpolacao, entidades remotas parecem "saltar" a cada tick.
//!
//! Solucao: renderizar o mundo 100ms no passado (render_delay). Sempre
//! temos dois snapshots para interpolar entre si.
//!
//! Buffer mantem os ultimos N snapshots ordenados por server_time_ms.
//! `sample(render_time)` encontra o par de snapshots mais proximo e lerp.

use shared::{EntityId, EntitySnapshot};
use std::collections::{HashMap, VecDeque};

/// Delay de render em ms. Quanto maior, mais suave mas mais "atrasado".
/// 10ms prioriza responsividade e ainda permite interpolacao entre snapshots.
pub const RENDER_DELAY_MS: u64 = 10;

/// Snapshot de mundo com timestamp.
#[derive(Clone)]
struct TimedSnap {
    time_ms: u64,
    entities: HashMap<EntityId, EntitySnapshot>,
}

pub struct InterpolationBuffer {
    snapshots: VecDeque<TimedSnap>,
    /// Maximo de snapshots guardados (~1s de historico a 30Hz).
    capacity: usize,
}

impl InterpolationBuffer {
    pub fn new() -> Self {
        Self { snapshots: VecDeque::new(), capacity: 32 }
    }

    /// Adiciona novo snapshot. Descarta os mais antigos se passou da capacidade.
    pub fn push(&mut self, time_ms: u64, entities: Vec<EntitySnapshot>) {
        let map: HashMap<EntityId, EntitySnapshot> =
            entities.into_iter().map(|e| (e.id, e)).collect();
        self.snapshots.push_back(TimedSnap { time_ms, entities: map });
        while self.snapshots.len() > self.capacity {
            self.snapshots.pop_front();
        }
    }

    /// Retorna entidades interpoladas para `render_time_ms`.
    /// Encontra os dois snapshots vizinhos e lerp entre eles.
    pub fn sample(&self, render_time_ms: u64) -> Vec<EntitySnapshot> {
        if self.snapshots.is_empty() {
            return Vec::new();
        }

        // Encontrar o par (before, after) que envolve render_time.
        let mut before = None;
        let mut after = None;
        for snap in &self.snapshots {
            if snap.time_ms <= render_time_ms {
                before = Some(snap);
            } else if after.is_none() {
                after = Some(snap);
                break;
            }
        }

        match (before, after) {
            (Some(b), Some(a)) => {
                let span = (a.time_ms - b.time_ms) as f32;
                let t = if span > 0.0 {
                    ((render_time_ms - b.time_ms) as f32 / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                lerp_snapshots(b, a, t)
            }
            // Ainda nao temos dois snapshots: usar o mais recente disponivel.
            (Some(b), None) => b.entities.values().cloned().collect(),
            (None, Some(a)) => a.entities.values().cloned().collect(),
            (None, None) => Vec::new(),
        }
    }
}

impl Default for InterpolationBuffer {
    fn default() -> Self { Self::new() }
}

fn lerp_snapshots(before: &TimedSnap, after: &TimedSnap, t: f32) -> Vec<EntitySnapshot> {
    let mut result = Vec::with_capacity(after.entities.len());
    for (id, b) in &before.entities {
        if let Some(a) = after.entities.get(id) {
            let mut interp = b.clone();
            interp.pos = b.pos.lerp(a.pos, t);
            interp.vel = b.vel.lerp(a.vel, t);
            result.push(interp);
        } else {
            // Entidade desapareceu no proximo snap: manter ate desaparecer.
            result.push(b.clone());
        }
    }
    // Entidades que so existem no snap mais recente (recentemente spawnadas).
    for (id, a) in &after.entities {
        if !before.entities.contains_key(id) {
            result.push(a.clone());
        }
    }
    result
}
