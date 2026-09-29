//! Measures the cost of one tick on the wire. `cargo run --bin wire_size`.
//!
//! It exists so the format decision is made with a number, not with intuition:
//! it is the mobile data cost per player that decides whether the game runs on a phone.

use shared::protocol::WorldSnapshot;
use shared::{EntityId, EntityMeta, EntityState, EntityTag};

fn meta(i: u32) -> EntityMeta {
    EntityMeta {
        pk: Default::default(),
        auras: 0,
        id: EntityId(1000 + i),
        tag: EntityTag::Enemy,
        name: Some("Green Goblin Lv3".to_string()),
        hp_max: 200,
        faction: None,
        kind: 0,
        nivel: 1,
                desafio: None,
            aparencia: 0,
            }
}

fn state(i: u32) -> EntityState {
    EntityState::quantize(
        EntityId(1000 + i),
        glam::Vec2::new(84.5, 92.25),
        glam::Vec2::new(0.5, -0.25),
        120,
        0,
    )
}

fn main() {
    let mbs = |b: usize| b as f64 * 30.0 / 1024.0 / 1024.0;
    println!(
        "state per entity: {} bytes",
        shared::protocol::encode(&state(0)).unwrap().len()
    );
    println!(
        "meta  por entidade: {} bytes (uma vez, na entrada do AOI)\n",
        shared::protocol::encode(&meta(0)).unwrap().len()
    );

    for n in [30u32, 100, 300, 1000] {
        // Steady-state tick: everyone has already joined, only the state repeats.
        let regime = WorldSnapshot {
            tick: 12345,
            server_time_ms: 1_700_000_000_000,
            last_input_seq: 42,
            entered: Vec::new(),
            states: (0..n).map(state).collect(),
            removed: Vec::new(),
            acertos: Vec::new(),
        };
        // Worst case: everyone joining at once (map change, teleport).
        let entrada = WorldSnapshot {
            entered: (0..n).map(meta).collect(),
            states: (0..n).map(state).collect(),
            ..WorldSnapshot {
                tick: 0,
                server_time_ms: 0,
                last_input_seq: 0,
                entered: Vec::new(),
                states: Vec::new(),
                removed: Vec::new(),
                acertos: Vec::new(),
            }
        };
        let r = shared::protocol::encode(&regime).unwrap().len();
        let e = shared::protocol::encode(&entrada).unwrap().len();
        println!(
            "{n:>4} entidades | regime {r:>6} B ({:>5.2} MB/s) | entrada {e:>6} B",
            mbs(r)
        );
    }
}
