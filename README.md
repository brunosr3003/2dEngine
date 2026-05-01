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
| ECS | `hecs` |
| Net | `tokio` + `tokio-tungstenite` (WebSocket binário) |
| Serialização | `rmp-serde` (MessagePack named) + `serde` |
| Matemática | `glam` |
| Persistência | `sqlx` + Postgres |
| Auth | `argon2` |
| Web (cadastro) | `axum` |

**Cliente:** Unity 6.4 + URP 2D (`/mmorpg`).

Escolhas explicadas em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Estrutura

```
2dEngine/
├── Cargo.toml              workspace
├── crates/
│   ├── shared/             protocolo + componentes (types compartilhados)
│   ├── server/             servidor autoritativo tokio
│   └── web/                cadastro / admin HTTP (axum)
└── docs/                   arquitetura, roadmap, networking
```

## Quick start

### 1. Instalar Rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Subir Postgres (dev)

```sh
# Requer DATABASE_URL no .env.local
# postgres://solar:solar_dev_123@localhost:5432/mmo_dev
```

### 3. Rodar servidor

```sh
cd 2dEngine
cargo run --bin server
# ws://0.0.0.0:9000 — tick 30Hz
```

### 4. Conectar cliente Unity

Abra o projeto `/Users/bruno/mmorpg` no Unity 6.4,
rode a cena `Login` e conecte em `ws://127.0.0.1:9000`.

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
- [Admin de Economia (web /econ.html)](docs/ADMIN_ECONOMY.md) — editar items, drops e mobs ao vivo (hot-reload)

## Licença

MIT OR Apache-2.0
