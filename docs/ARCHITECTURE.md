# Arquitetura

## Princípios

1. **Servidor é autoridade.** Clientes mandam *intent*, servidor simula e
   replica. Clientes podem *prever* para responsividade, mas a verdade
   vem do servidor.
2. **Engine não sabe de rede.** Rede é do cliente/servidor. Isso mantém a
   engine portável (wasm, iOS, Android) sem puxar `tokio`.
3. **Compartilhado vai em `shared`.** Protocolo, constantes de mundo e
   componentes usados pelos dois lados ficam em um único crate sem deps
   de plataforma.
4. **ECS-first.** `hecs` é leve, sem magia. Tudo que é entidade (jogador,
   mob, projétil, loot) é entidade — evita hierarquias de classes.

## Diagrama

```
┌─────────────────────────────────────────────────────────────┐
│                         shared                              │
│  (protocol, components, constants)  ───────────────┐        │
└─────────────────────────────────────────────────────┼────────┘
                                                     │
                  ┌──────────────────────────────────┤
                  │                                  │
        ┌─────────▼─────────┐              ┌─────────▼─────────┐
        │      engine       │              │      server       │
        │  render (wgpu)    │              │  tokio runtime    │
        │  input (winit)    │              │  websocket accept │
        │  ecs   (hecs)     │              │  world loop (tick)│
        │  time, assets,    │              │  ecs (hecs)       │
        │  math             │              │  [persistence →]  │
        └─────────▲─────────┘              └───────────────────┘
                  │
        ┌─────────┴─────────┐
        │      client       │
        │  net_client (ws)  │
        │  game state       │
        └───────────────────┘
```

## Camadas

### `shared` (leaf, zero-dep)

- `protocol.rs`: enums `ClientMessage` / `ServerMessage` + helpers bincode.
- `components.rs`: `Position`, `Velocity`, `Health`, `EntityKind`,
  `EntitySnapshot`. Usados pelo servidor (componentes hecs) e pelo cliente
  (replicação).
- `constants.rs`: `TICK_RATE_HZ`, `AOI_RADIUS`, `PROTOCOL_VERSION`.

Qualquer mudança aqui potencialmente quebra o protocolo — bumpar versão.

### `engine`

Wrapper fino sobre wgpu/winit/hecs/glam/image. Zero lógica de jogo.

- `app::run(config, game)` dá o event loop. `Game` trait é o contrato:
  `init`, `update(dt)`, `render(batch, alpha)`, `shutdown`.
- `render::Renderer`: um pipeline WGSL para sprites instanciados
  (1 draw-call por frame com milhares de sprites).
- `render::Camera2D`: ortográfica 2D, Y-up; `screen_to_world()` para
  converter mouse/touch.
- `input::Input`: estado frame-based, edge-triggered
  (`key_pressed` vs `key_down`).
- `time::FixedTimestep`: `advance()` retorna N ticks; `alpha()` para
  interpolação entre ticks (smooth 60/144/... FPS render).
- `assets::{ImageData, Atlas}`: carregamento de PNG + UVs por sub-retângulo.

### `server`

- `main.rs`: `accept` loop aceitando TCP → WebSocket → spawn por sessão.
- `session.rs`: I/O da sessão. Decodifica `ClientMessage`, envia para o
  `world` via mpsc. Recebe `ServerMessage` do world e escreve no socket.
- `world.rs`: `GameWorld` com ECS e `sessions`. Dono de todos os dados.
- `tick.rs`: `run_world_loop` — fixed-tick (30 Hz). Drena mensagens,
  simula, envia snapshots.

**Sem locks**: o mundo é dono exclusivo do seu estado. Comunicação
cross-task é só via canais.

### `client`

- `main.rs`: implementa `engine::app::Game`. Gerencia estado de conexão,
  coleta input, renderiza snapshots recebidos.
- `net_client.rs`: spawn thread com runtime tokio mínimo que mantém a
  WebSocket. Dois canais (in/out) para comunicar com o jogo.

## Modelo de rede (resumo)

Ver [NETWORKING.md](NETWORKING.md) para o detalhado.

- **Autoridade:** servidor.
- **Transporte:** WebSocket binário (TCP). UDP+QUIC em estudo para Fase 5.
- **Serialização:** bincode (pequeno, rápido, estável).
- **Replicação:** snapshot de AOI a cada tick.
- **Predição:** cliente-side (roadmap) com reconciliação via
  `WorldSnapshot::last_input_seq`.

## Escolhas explicadas

### Por que wgpu?

Único backend que cobre **todos** os targets que queremos: Vulkan (Linux,
Android), Metal (macOS, iOS), DX12 (Windows), WebGPU (navegadores modernos),
GL (fallback). Sem ele cairíamos em SDL/OpenGL e perderíamos web.

### Por que winit?

Mesmo: único que abstrai janela/input em todos os targets inclusive
Android (via `android-activity`), iOS e web.

### Por que hecs e não bevy_ecs?

`hecs` é simples, pequeno e sem "mágica de scheduler" — pra este projeto
não precisamos de paralelismo automático. Se crescer, trocar é tranquilo
porque a lógica está toda em funções livres que tomam `&mut World`.

### Por que WebSocket e não UDP?

- **Portabilidade:** WebSocket funciona em TODOS os targets, incluindo
  browser sem workaround. UDP precisa WebRTC data-channel no browser
  (complexo) ou nativo puro.
- **TCP head-of-line:** verdade, mas para MMO-2D com 30Hz de snapshot e
  AOI, a latência perceptível é aceitável.
- **Plano B:** na Fase 5 avaliaremos QUIC (quinn) para clientes nativos,
  com WebSocket só no browser. Protocolo no `shared` fica transport-agnóstico.

### Por que tick fixo a 30Hz?

Balanço entre banda (payload por segundo) e responsividade. Predição
client-side absorve a latência entre ticks. 60Hz dobraria a banda sem
ganho perceptível para este estilo de jogo.
