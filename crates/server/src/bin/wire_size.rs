//! Mede o custo de um tick no wire. `cargo run --bin wire_size`.
//!
//! Existe pra a decisao de formato ser tomada com numero, nao com intuicao:
//! e' o consumo de dado movel por jogador que decide se o jogo roda no celular.

use shared::protocol::WorldSnapshot;
use shared::{EntityId, EntityMeta, EntityState, EntityTag};

fn meta(i: u32) -> EntityMeta {
    EntityMeta {
        id: EntityId(1000 + i),
        tag: EntityTag::Enemy,
        name: Some("Green Goblin Lv3".to_string()),
        hp_max: 200,
        faction: None,
        kind: 0,
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
    println!("estado por entidade: {} bytes", shared::protocol::encode(&state(0)).unwrap().len());
    println!("meta  por entidade: {} bytes (uma vez, na entrada do AOI)\n",
        shared::protocol::encode(&meta(0)).unwrap().len());

    for n in [30u32, 100, 300, 1000] {
        // Tick de regime: todo mundo ja entrou, so' o estado se repete.
        let regime = WorldSnapshot {
            tick: 12345,
            server_time_ms: 1_700_000_000_000,
            last_input_seq: 42,
            entered: Vec::new(),
            states: (0..n).map(state).collect(),
            removed: Vec::new(),
        };
        // Pior caso: todo mundo entrando de uma vez (troca de mapa, teleporte).
        let entrada = WorldSnapshot {
            entered: (0..n).map(meta).collect(),
            states: (0..n).map(state).collect(),
            ..WorldSnapshot { tick: 0, server_time_ms: 0, last_input_seq: 0,
                              entered: Vec::new(), states: Vec::new(), removed: Vec::new() }
        };
        let r = shared::protocol::encode(&regime).unwrap().len();
        let e = shared::protocol::encode(&entrada).unwrap().len();
        println!("{n:>4} entidades | regime {r:>6} B ({:>5.2} MB/s) | entrada {e:>6} B",
            mbs(r));
    }
}
