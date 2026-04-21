# 2dEngine

Engine 2D própria em Rust + MMORPG 2D multi-plataforma (PC, mobile, web).
Estilo visual alvo: **Dark Fantasy / Gritty 2D** — visão top-down, foco em 
sobrevivência, PvP de alto risco, sistema sem classes e combates tensos (hardcore).
> **Status:** scaffold inicial. Compila e roda um loop end-to-end (cliente
> conecta, servidor simula em tick fixo, entidades são replicadas). Falta
> muita coisa — ver [docs/ROADMAP.md](docs/ROADMAP.md).

## Stack

| Camada | Tech |
|---|---|
| Render | `wgpu` (Vulkan/Metal/DX12/GL/WebGPU) |
| Janela/input | `winit` (Windows/macOS/Linux/Android/iOS/Web) |
| ECS | `hecs` |
| Net (native) | `tokio` + `tokio-tungstenite` (WebSocket binário) |
| Net (wasm) | `web-sys::WebSocket` *(planejado)* |
| Serialização | `bincode` + `serde` |
| Matemática | `glam` |

Escolhas explicadas em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Estrutura

```
2dEngine/
├── Cargo.toml              workspace
├── crates/
│   ├── engine/             engine (render, input, ECS, assets)
│   ├── shared/             protocolo + componentes cliente/servidor
│   ├── server/             servidor autoritativo tokio
│   └── client/             cliente (usa engine)
├── docs/                   arquitetura, roadmap, networking
└── assets/                 sprites, tilemaps, sons
```

## Quick start

### 1. Instalar Rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# siga as instruções; depois: source "$HOME/.cargo/env"
```

Rust 1.82+ recomendado (definido em `rust-toolchain.toml`).

### 2. Rodar servidor

```sh
cd 2dEngine
cargo run --bin server
# server listening on ws://0.0.0.0:9000 (30 Hz)
```

### 3. Rodar cliente (em outro terminal)

```sh
cargo run --bin client
# abre uma janela, conecta em ws://127.0.0.1:9000
# WASD move, Esc sai
```

Múltiplos clientes na mesma máquina:

```sh
USERNAME=Alice cargo run --bin client
USERNAME=Bob   cargo run --bin client
```

## Próximas fases

Detalhes em [docs/ROADMAP.md](docs/ROADMAP.md). Resumo:

- **Fase 1 (atual):** scaffold — loop end-to-end.
- **Fase 2:** render de atlas de sprites + tilemap + predição client-side.
- **Fase 3:** combate (projéteis, dano, morte), NPCs com IA, loot.
- **Fase 4:** persistência (SQLite/Postgres), auth, inventário, classes.
- **Fase 5:** cross-platform — mobile (Android/iOS) + web (wasm).
- **Fase 6:** conteúdo (biomas, dungeons, bosses) + polish.

## Documentação

- [Arquitetura geral](docs/ARCHITECTURE.md)
- [Networking (protocolo, replicação, predição)](docs/NETWORKING.md)
- [Build (nativo, Android, iOS, web)](docs/BUILD.md)
- [Gameplay design](docs/GAMEPLAY.md)
- [Roadmap detalhado](docs/ROADMAP.md)

## Licença

MIT OR Apache-2.0
