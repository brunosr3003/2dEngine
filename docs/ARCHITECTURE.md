# Arquitetura

> Atualizado: 2026-04-24
>
> **Mudança histórica:** o crate `engine/` (renderer wgpu + winit + hecs no cliente)
> foi removido. O cliente oficial agora é **Unity 6.4** (repo separado em
> `github.com/brunosr3003/MMORPG`). Este workspace mantém somente a stack
> autoritativa em Rust: servidor + protocolo + web de registro.

## Princípios

1. **Servidor é autoridade.** Clientes mandam *intent* (inputs, requests),
   servidor simula e replica. Unity pode prever movimento local para
   responsividade, mas a verdade vem do servidor via `WorldSnapshot`.
2. **Compartilhado fica em `shared`.** Protocolo, constantes de mundo,
   physics helpers e world-gen determinístico ficam em um único crate sem
   deps de plataforma — tanto o servidor quanto (futuramente) um cliente
   Rust ou ferramenta de análise podem reusar.
3. **ECS-first no servidor.** `hecs` é leve, sem magia. Tudo que é entidade
   (jogador, mob, projétil, loot, NPC) é entidade.
4. **Classless + diferenciação por equip/proficiência.** Stats base iguais
   pra todo mundo; progressão vem de alocar pontos e subir proficiências
   de arma. Regras de cálculo ficam em `shared` pra manter client/server
   consistentes.

## Diagrama

```
┌──────────────────────────────────────────────────────────────┐
│                          shared                              │
│  protocol · components · constants · physics · world_gen     │
└──────────────────────────────┬───────────────────────────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
     ┌────────▼────────┐  ┌────▼────┐  ┌────────▼─────────┐
     │     server      │  │   web   │  │  Unity (externo) │
     │  tokio runtime  │  │ axum    │  │  ClientWebSocket │
     │  websocket :9000│  │ :8090   │  │  URP 2D + HUD    │
     │  tick 30Hz      │  │ register│  │  paper doll      │
     │  ecs (hecs)     │  │ argon2id│  │  inv/equip/vault │
     │  persistence PG │  │         │  │  Cinemachine     │
     └────────┬────────┘  └────┬────┘  └──────────────────┘
              │                │
              └──────┬─────────┘
                     ▼
              ┌─────────────┐
              │  Postgres   │
              │   mmo_dev   │
              └─────────────┘
```

## Camadas

### `shared` (leaf, sem deps de plataforma)

- `protocol.rs`: enums `ClientMessage` / `ServerMessage` com `#[serde(tag="type")]`
  (externally-tagged). Inclui mensagens de rede, combate, economia (shop/vault),
  progressão (`AllocStatPoint`, `StatPointsUpdate`), social (party), e helpers de
  serialização (`vec2_arr` trata `Vec2` como `[x, y]`).
- `components.rs`: `Position`, `Velocity`, `Health`, `EntityKind`,
  `EntitySnapshot`, `InventorySlot`, `ShopItem`, etc. Usados pelo servidor
  como componentes hecs e atravessam o wire como parte do protocolo.
- `constants.rs`: `PROTOCOL_VERSION` (atual: **24**), `TICK_RATE_HZ`,
  `AOI_RADIUS`, `MELEE_RANGE`, `STAT_POINT_BONUS`, `POINTS_PER_LEVEL`,
  `INTERACT_RADIUS`, etc.
- `physics.rs`: AABB discreta por eixo (`move_and_slide`), `overlaps_solid`.
- `world_gen.rs`: geração procedural determinística (rooms + corridors +
  dungeon room) — rodada só no servidor hoje, mas poderia ser usada por
  ferramentas offline.
- `mapfile.rs`: formato de mapa manual carregável.

> **Regra:** qualquer mudança em `protocol.rs` ou nos wire types precisa bumpar
> `PROTOCOL_VERSION`. O servidor rejeita no `Handshake`.

### `server`

- `main.rs`: bootstrap (tokio runtime, loga config, conecta Postgres,
  starta `accept` loop WebSocket em `:9000`).
- `auth.rs`: verificação de senha com argon2id (re-hash on login quando
  necessário).
- `persistence.rs`: todo I/O com Postgres (`accounts`, `characters`,
  `inventory`, `equipment`, `vault`, `proficiencies`, `allocated_points`).
  Batch writes via snapshot periódico do `GameWorld`.
- `session.rs`: I/O de cada sessão WebSocket. Decodifica `ClientMessage`,
  envia via mpsc para o world loop. Recebe `ServerMessage` do world e
  escreve no socket.
- `world.rs`: `GameWorld` — dono exclusivo do ECS, inimigos, projéteis,
  NPCs (Vendor=Npc(1), Vault=Npc(2)), shop items, AOI, state de party.
  TODO v2 comentado: `maps: HashMap<String, MapInstance>` para multi-map.
- `tick.rs`: `run_world_loop` — fixed-tick 30Hz. Drena mensagens, simula
  physics/IA/combate, gera snapshots ego-centric por sessão, envia.

**Concorrência:** o world loop é single-task (sem locks no ECS).
Toda comunicação cross-task é por canais mpsc. I/O de sessão roda em
tasks separadas.

### `web`

- Axum HTTP em `:8090`. Endpoint `/api/register` cria conta com argon2id
  (hash armazenado em `accounts.password_hash`). Stateless; o login real
  acontece depois no WebSocket via `ClientMessage::Login`.

### Cliente Unity (externo)

Documentado em detalhe no repo `MMORPG`. Resumo do que importa para a
arquitetura do servidor:

- `ClientWebSocket` nativo (NÃO NativeWebSocket) em `NetClient.cs`.
- Serialização com Newtonsoft.Json, compatível com `#[serde(tag="type")]`
  do lado Rust.
- Envia `InputFrame` (intent) por tick; aplica predição local e reconcilia
  via `WorldSnapshot::last_input_seq`.
- Todo estado de UI (inventário, equip, vault, party, proficiências,
  stats) vem por mensagens dedicadas — não é inferido.

## Modelo de rede

Ver [NETWORKING.md](NETWORKING.md) para o detalhado.

- **Autoridade:** servidor.
- **Transporte:** WebSocket sobre TCP.
- **Serialização:** **JSON externally-tagged** via serde (Rust) ↔ Newtonsoft
  (Unity). Escolha deliberada pra debuggability e compat com browser; a
  troca por bincode/messagepack está no roadmap de perf.
- **Replicação:** `WorldSnapshot` ego-centric por tick com `AOI_RADIUS=24u`.
- **Reconciliação:** `WorldSnapshot::last_input_seq` permite ao cliente
  re-simular inputs pendentes.
- **AOI:** hoje O(E×S) naïve por raio — spatial hash grid está planejado.

## Escolhas explicadas

### Por que Unity como cliente em vez de um cliente Rust?

O crate `engine/` (wgpu + winit + hecs) funcionou pra fase de scaffold,
mas:

1. **Ferramental.** Unity dá editor, animation system, Cinemachine,
   TextMeshPro, InputSystem, build pipeline multi-plataforma — meses de
   trabalho economizados.
2. **Paper doll e assets Mana Seed.** Sprite layering, SpriteRenderer
   ordering, SortingGroup, Tilemap, 2D URP Global Light estão prontos.
3. **Cross-platform sem atrito.** Windows/Linux/WebGL/Android/iOS via
   Unity em vez de implementar winit + wgpu em cada target.

O tradeoff é perder Rust no cliente (menos compartilhamento de código com
o servidor), mas com o protocolo em `shared` e regras determinísticas
documentadas, a duplicação é pequena.

### Por que WebSocket e não UDP/QUIC?

- **Portabilidade.** WebSocket roda em TODOS os targets, inclusive browser
  sem workaround. QUIC em browser exige WebRTC DataChannel (complexo).
- **TCP head-of-line é real** mas a 30Hz de snapshot, pra MMO-2D AOI-based,
  a latência é aceitável.
- **Plano B:** avaliar QUIC (quinn) pra clientes nativos no futuro,
  com WebSocket fallback pra WebGL. Protocolo em `shared` é
  transport-agnóstico.

### Por que JSON e não bincode?

- **Debuggability.** Inspeção no browser devtools / Wireshark / logs.
- **Compat com Newtonsoft.** Unity usa Newtonsoft, que mapeia direto em
  `#[serde(tag="type")]` externally-tagged sem custom converters.
- **Custo:** ~3–5× mais bytes que bincode. Não dói em dev; troca planejada
  pra fase de perf (bincode ou messagepack, com quantização de posição).

### Por que tick fixo a 30Hz?

Balanço entre banda (payload por segundo) e responsividade. Predição
client-side absorve a latência entre ticks. 60Hz dobraria a banda sem
ganho perceptível pra este estilo de jogo.

### Por que hecs e não bevy_ecs?

`hecs` é simples, pequeno, sem scheduler automático — pra este servidor
single-task não precisamos de paralelismo. Se crescer (sharding por mapa),
a lógica está em funções livres sobre `&mut World`, então trocar é viável.
