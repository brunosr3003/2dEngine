# Implementation Status

> Atualizado: 2026-04-20
> Convenção: ✅ Feito · 🚧 Em progresso · ⬜ Planejado · ❌ Bloqueado

---

## Engine (`crates/engine`)

### Render
- ✅ Pipeline wgpu com sprite batch instanciado (1 draw-call, ~8k sprites)
- ✅ Shader WGSL — sprites com rotação, tint, flip UV
- ✅ Camera 2D ortográfica — zoom, `screen_to_world`, `visible_rect`
- ✅ `SpriteBatch` — push/clear, upload GPU por frame
- ✅ `Tilemap` — TileDef (UV + solid), `fill_batch` com camera culling
- ✅ `BitmapFont` — texto pixel-art via SpriteBatch (escala, newline, measure)
- ✅ `DebugDraw` — rect, outline, line, circle, cross, point (via SpriteBatch)
- 🚧 Atlas de sprites real carregado (aguarda Fase 2 no client)
- ⬜ Z-ordering / y-sort por entidade
- ⬜ Tilemap em chunks (para mapas > 256×256)
- ⬜ Post-processing (screen shake, flash de dano, vignette)
- ⬜ Minimapa (render offscreen)

### Animação
- ✅ `AnimFrame`, `AnimClip`, `AnimRegistry`
- ✅ `AnimPlayer` (componente ECS) — `update`, `uvs`, `flip_x`, `set_clip`
- ✅ `from_row` helper para spritesheets em linha
- ⬜ Blend de transição entre clips
- ⬜ Eventos de frame (trigger de SFX no frame N)

### Input
- ✅ Teclado — `key_down`, `key_pressed`, `key_released`
- ✅ Mouse — posição, botões, scroll
- ✅ `move_vector()` WASD / setas
- ⬜ Touch — múltiplos toques, joystick virtual
- ⬜ Gamepad (gilrs)
- ⬜ Text input / IME (para chat)

### Física / Colisão
- ✅ `move_and_slide` AABB discreta por eixo vs tilemap
- ✅ `overlaps_solid` query
- ⬜ Colisão entidade vs entidade (broadphase + narrow)
- ⬜ Swept AABB para projéteis rápidos (anti-tunneling)

### Audio
- ✅ `AudioManager` (rodio) — `play_sfx`, `play_music`, `stop`, `pause`, `resume`
- ✅ Volume master / música / sfx independentes
- ✅ Stub wasm (compila limpo, no-op)
- ⬜ Spatial audio (volume por distância)
- ⬜ Pool de fontes (limitar simultâneas)

### Assets
- ✅ `AssetManager` — `ImageHandle` (Arc), `load_image`, `load_raw`
- ✅ `Atlas` — sub-retângulos por nome
- ⬜ Loading assíncrono (background thread)
- ⬜ Hot reload em dev mode
- ⬜ Parser LDtk / Tiled para tilemaps externos

### App / Loop
- ✅ `Game` trait — `init(renderer)`, `update(dt)`, `render(batch, alpha)`, `shutdown`
- ✅ `FixedTimestep` — tick fixo com alpha de interpolação e anti-spiral-of-death
- ✅ Event loop winit (resize, close, redraw)
- ⬜ Fullscreen toggle
- ⬜ Lifecycle pause/resume (Android/iOS)
- ⬜ Config persistido (volume, resolução, keybindings)

---

## Server (`crates/server`)

- ✅ Servidor tokio + WebSocket autoritativo
- ✅ Tick loop 30 Hz (single-task, sem locks no ECS)
- ✅ Sessões isoladas via mpsc (I/O separado do mundo)
- ✅ Handshake com verificação de versão de protocolo
- ✅ Login, Logout, Chat, InputFrame, WorldSnapshot
- ✅ AOI naive por raio — filtra entidades fora do alcance
- ⬜ Spatial hash grid (O(E + ΣAOI) — substitui O(E×S))
- ⬜ Colisão server-side contra tilemap compartilhado
- ⬜ Spawner de NPCs / inimigos com seed
- ⬜ Projéteis server-side (spawn, movimento, colisão, TTL)
- ⬜ Sistema de combate (dano, morte, respawn)
- ⬜ Loot (drop ao morrer, coleta por proximidade)
- ⬜ Persistência — SQLite dev / Postgres prod
- ⬜ Auth real (argon2, JWT) — hoje só stub "dev"
- ⬜ Rate-limit por sessão
- ⬜ Sharding (múltiplos mundos em processos separados)

---

## Client (`crates/client`)

- ✅ WebSocket nativa (tokio-tungstenite em thread dedicada)
- ✅ Handshake + login + desconexão limpa
- ✅ `InterpolationBuffer` — 100ms delay, lerp entre snapshots
- ✅ `PredictionBuffer` — re-simula inputs pendentes, correção suave (8 frames) ou snap
- ✅ Camera smooth-follow no próprio jogador
- 🚧 Atlas de dev carregado e enviado ao renderer
- 🚧 Tilemap com mundo procedural (rooms + corridors)
- 🚧 `AnimPlayer` ligado às entidades renderizadas
- 🚧 Nomes acima dos jogadores (`BitmapFont`)
- 🚧 HUD básico (coordenadas, instrução de teclas)
- ⬜ HP bar acima de cada entidade
- ⬜ Chat UI (caixa de entrada + histórico)
- ⬜ Minimapa
- ⬜ Inventário / hotbar
- ⬜ Tela de login / seleção de personagem
- ⬜ Tela de morte / respawn

---

## Protocolo (`crates/shared`)

- ✅ `ClientMessage`: Handshake, Login, Input, Chat, RequestDisconnect
- ✅ `ServerMessage`: HandshakeAck, LoginOk, LoginDenied, Snapshot, Chat, Kick
- ✅ `WorldSnapshot` com `last_input_seq` para reconciliação
- ✅ `EntitySnapshot` com campos Option para delta futuro
- ✅ `PROTOCOL_VERSION` — servidor rejeita versão diferente
- ✅ Constantes de mundo: `TICK_RATE_HZ`, `AOI_RADIUS`, `PLAYER_SPEED`, etc.
- ⬜ Eventos de mundo: spawn, death, damage, loot (hoje tudo no Snapshot)
- ⬜ Delta snapshots (só o que mudou vs snapshot anterior acked)
- ⬜ Quantização de posição (Vec2 → fixed-point 16-bit, ~50% de banda)
- ⬜ Compressão (zstd sobre bincode)

---

## Cross-platform

- ✅ macOS (Apple Silicon) — compilando e rodando
- ⬜ Windows — wgpu DX12/Vulkan
- ⬜ Linux — wgpu Vulkan
- ⬜ Web — wasm32 + WebGPU + web-sys WebSocket
- ⬜ Android — cargo-apk + NativeActivity + touch input
- ⬜ iOS — cargo-mobile2 + Metal

---

## Infra / CI

- ⬜ GitHub Actions: `cargo check` + `cargo test` + `cargo clippy` por PR
- ⬜ `cargo deny` (licenças + CVEs)
- ⬜ Docker para o servidor
- ⬜ Observabilidade: tracing → OTLP → Grafana

---

## Roadmap de Fases

| Fase | Status | Goal |
|------|--------|------|
| **1 — Scaffold** | ✅ | Loop end-to-end: cliente conecta, quadrados coloridos, servidor simula |
| **2 — Mundo Jogável** | 🚧 | Mapa com tiles, sprites reais, animações, HUD básico, nomes |
| **3 — Combate** | ⬜ | Projéteis, dano, morte, NPCs com IA, loot |
| **4 — Persistência** | ⬜ | Contas, personagens, inventário, banco de dados |
| **5 — Cross-platform** | ⬜ | Android, iOS, Web (wasm) |
| **6 — Conteúdo** | ⬜ | Biomas, dungeons, bosses, economia, polish |
