# Tempest

An open-world MMO in Rust: authoritative server, purpose-built macroquad
client, and an archipelago of voxel islands generated from a seed.

```
crates/
  shared/      rules and terrain — what both sides MUST agree on
  server/      authoritative simulation, one process per channel
  client/      3D client (macroquad/miniquad)
  web/         signup, login and channel directory
  panoptico/   observability dashboard (read-only)
  admin_cli/   operations tooling
tools/voxrender/  voxel art → sprite pipeline
```

## What defines this project

**The terrain never travels over the network.** Client and server generate the
same island from the same seed, with the same code in `shared::terreno`. The
server keeps 2 bytes per column (~16 MB/km² against ~416 MB for a full volume);
the client builds the volume only around the player. What goes over the wire is
position, and nothing else.

**Collision is an integer comparison.** There is no navigation mesh and no 3D
collision: a one-block rise is walkable, two to three need a jump, four is a
wall. Going down is free. Trunks, thickets and stumps block; flowers and grass
do not.

**The client does not predict.** It smooths. Every game decision — moving,
jumping, attacking, pathfinding — happens on the server, and the client draws
the result. See [docs/NETWORKING.md](docs/NETWORKING.md).

**One truth per question.** When two parts of the code answer the same question
by different arithmetic, they drift — and the symptom shows up far from the
cause. This repository's history is made of that: the way a tree is drawn and
the way it collides, jump height and the step rule, the camera and hill
avoidance. Each one was a bug with the same shape, and the fix was always the
same: a single function, called by both sides.

## Running

```bash
# database + web (signup, channel list)
cargo run --release --bin web

# a single channel
MMO_ZONA=ilha_inicial BIND_ADDR=0.0.0.0:9200 cargo run --release --bin server

# the client, side by side with the terminal
./scripts/run-client.sh --build
```

Build details in [docs/BUILD.md](docs/BUILD.md); running channels in
[docs/SERVIDORES_E_CANAIS.md](docs/SERVIDORES_E_CANAIS.md).

Most documents under `docs/` are written in Portuguese.

## Documents

| Where | What |
|---|---|
| [ARCHITECTURE](docs/ARCHITECTURE.md) | the pieces and why they are apart |
| [MUNDO](docs/MUNDO.md) | island generation, movement, vegetation |
| [NETWORKING](docs/NETWORKING.md) | protocol, AOI, what goes over the wire |
| [SERVIDORES_E_CANAIS](docs/SERVIDORES_E_CANAIS.md) | realm, channel, auto-scaling |
| [ECONOMIA](docs/ECONOMIA.md) / [ITENS](docs/ITENS.md) | currency, forge, grade and refinement |
| [GAMEPLAY](docs/GAMEPLAY.md) / [NEW_MECHANICS](docs/NEW_MECHANICS.md) | design |
| [PANOPTICO](docs/PANOPTICO.md) | observability dashboard |
| [PIPELINE_ARTE](docs/PIPELINE_ARTE.md) | voxel → sprite |
| [COMBATE](docs/COMBATE.md) | weapon sets, armor weight, skills |
| [SKILLS](docs/SKILLS.md) | the 12 abilities and their level gates |
| [COLETA](docs/COLETA.md) | auto-gathering driven by local density |
| [ECONOMIA_DE_CRAFT](docs/ECONOMIA_DE_CRAFT.md) | materials, recipes, color synthesis |
| [PERSONAGEM](docs/PERSONAGEM.md) | rig, animation, mounts |
| [character create](docs/character%20create.md) | what to model: parts, sizes, pivots, palette |
| [COMBATE_POR_ALVO](docs/COMBATE_POR_ALVO.md) | targeted combat and the binary wire |
| [BOSSES](docs/BOSSES.md) | field bosses, telegraphed attacks, placement and respawn |
| [VILA_E_PORTO](docs/VILA_E_PORTO.md) | town, port, voxel houses, doorway NPCs |
| [MISSOES](docs/MISSOES.md) | Quest Master, the starting island's chain, journal (J) |
| [HISTORIA](docs/HISTORIA.md) | the endless main quest: chapters per island, level gates, Chronicles of the Tempest |
| [DUNGEONS_E_RAIDS](docs/DUNGEONS_E_RAIDS.md) | design: dungeons, raids, bosses, cross-realm matchmaking, rewards by band |
| [HUD](docs/HUD.md) | HUD and main menu in the MIR4 mold: layout, panels, nothing opens by keystroke |
| [RELEASE_MAC](docs/RELEASE_MAC.md) | Mac signing, notarization, and why `xattr` was needed |
| [MODO_ECONOMIA](docs/MODO_ECONOMIA.md) | power-saving mode: what keeps running behind the black screen |

## Stack

`hecs` (ECS) · `tokio` + binary WebSocket · `postcard` · `glam` ·
`sqlx`/Postgres · `argon2` · `axum` · `macroquad`

## License

Proprietary — all rights reserved. The code is public as a portfolio only;
there is no permission to use, copy, modify, distribute or host it. Third-party
material under `vendor/` and `assets/` keeps its own licenses. See
[LICENSE](LICENSE).
