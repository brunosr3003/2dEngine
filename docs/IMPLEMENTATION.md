# Implementation Status

> Atualizado: 2026-04-24
> Convenção: ✅ Feito · 🚧 Em progresso · ⬜ Planejado · ❌ Bloqueado
>
> **Nota arquitetural:** o crate `engine/` (renderer wgpu + audio + tilemap client-side) foi **removido**.
> O cliente agora é o projeto Unity em `github.com/brunosr3003/MMORPG` (repo separado).
> Este workspace contém apenas servidor + protocolo + web de registro.

---

## Server (`crates/server`)

### Núcleo
- ✅ Tokio + WebSocket autoritativo, tick 30 Hz single-task (sem locks no ECS)
- ✅ Sessões isoladas via mpsc (I/O separado do world loop)
- ✅ Handshake com verificação de `PROTOCOL_VERSION` (atual: **24**)
- ✅ Auth real com argon2id (`auth.rs`) — web server registra contas em `/api/register`
- ✅ Persistência Postgres (`persistence.rs`) — contas, personagens, inventário, equip, vault, stats alocados, proficiências
- ⬜ Rate-limit por sessão (flood protection)
- ⬜ Sharding (múltiplos mundos em processos separados)

### Mundo / Simulação
- ✅ World gen procedural (rooms + corridors + dungeon room no canto inferior-direito)
- ✅ AOI ego-centric por raio (AOI=24u) — filtra entidades fora do alcance
- ✅ Spawner de inimigos (Grunt / Tank / Ninja / Berserker / Boss com `defense` fixo)
- ✅ IA: wander + chase + ataque com cooldown
- ✅ Projéteis server-side (spawn, movimento, colisão, TTL)
- ✅ Combate: `dano = max(1, dmg_incoming - defense)` pra player e inimigo
- ✅ Melee cone attack (`MELEE_RANGE=1.8`, `π/3` half-angle) para SWORD/GREAT_SWORD/DAGGER/unarmed
- ✅ Morte de inimigos → loot drop; morte de player → Downed State (sem morte imediata) + drop inv/equip
- ✅ Dungeon acessível só via portais (nexus ↔ dungeon room)
- ⬜ Spatial hash grid (hoje AOI é O(E×S) naïve — não escala além de ~100 entidades)
- ⬜ Colisão server-side contra tilemap compartilhado (cliente pode atravessar parede se mentir)
- ⬜ Swept AABB para projéteis rápidos (anti-tunneling)
- ⬜ Colisão entidade vs entidade (broadphase + narrow)

### Progressão
- ✅ Level up → `POINTS_PER_LEVEL=3` pontos livres; alocação via `AllocStatPoint { stat }`
- ✅ Stats: hp_max, mp_max, atk, dex, wis, res (bonus `STAT_POINT_BONUS`)
- ✅ `effective_stats(equip, allocated, profs)` soma base + alocado + scaling da arma × prof_level
- ✅ Proficiências passivas (Sword=hp+res, Espadão=atk, Dagger=atk+dex, Staff=mp+wis, Bow=dex+atk, Wand=mp+wis)

### NPCs / Economia
- ✅ Npc(1) = Vendor → `ShopOpen`/`ShopBuy`/`ShopClose`
- ✅ Npc(2) = Vault → `VaultOpen`/`VaultDeposit`/`VaultWithdraw`/`VaultClose` (24 slots)
- ✅ `ClientMessage::Interact` standalone (bit `Buttons.Interact` é ignorado pelo server)
- ✅ `INTERACT_RADIUS = 3.0`

### Social
- ✅ Party: convite via `/party <nome>` ou `PartyInvite`, aceitar/recusar, `PartyUpdate { members }`, sair
- ✅ Chat bidirecional + mensagens de sistema (`Chat { from: "SYS" }`, `Kick { reason }`)

---

## Protocolo (`crates/shared`)

### Mensagens
- ✅ **Client→Server**: Handshake, Login, Input, Chat, Ping, UseItem, Interact, ShopBuy, VaultDeposit/Withdraw/Close, InventorySwap, StandUp, TeleportToVendor, PartyInvite/Accept/Decline/Leave, AllocStatPoint, RequestDisconnect
- ✅ **Server→Client**: HandshakeAck, LoginOk/Denied, Snapshot, Chat, Pong, MapChange, ProgressUpdate, InventoryUpdate, StatsUpdate, ManaUpdate, StaminaUpdate, ShopOpen/Close, VaultOpen/Update/Close, DownedUpdate, FameUpdate, AuraUpdate, ProficienciesUpdate, PartyInviteReceived, PartyUpdate, StatPointsUpdate, Kick

### Transporte
- ✅ `PROTOCOL_VERSION` bump obrigatório ao mudar mensagens (servidor rejeita versão diferente)
- ✅ Serialização JSON externally-tagged (`tag="type"` via serde) — compatível com Newtonsoft no Unity
- ✅ `Vec2` serializado como `[x, y]` via `shared::vec2_arr`
- ✅ `WorldSnapshot` com `last_input_seq` para reconciliação
- ⬜ Trocar JSON por bincode/messagepack (maior ganho de banda possível)
- ⬜ Delta snapshots (só o que mudou desde o último ack) — hoje manda full-state a 30Hz
- ⬜ Quantização de posição (Vec2 → fixed-point 16-bit, ~50% banda)
- ⬜ Compressão (zstd sobre o wire format)
- ⬜ Eventos de mundo discretos (spawn/death/damage/loot fora do Snapshot)

---

## Client (Unity — repo `MMORPG`)

> Cliente oficial. Detalhes e scripts em `github.com/brunosr3003/MMORPG` (Unity 6.4 + URP 2D).

### Rede
- ✅ `ClientWebSocket` nativo (NÃO NativeWebSocket) em `NetClient.cs`
- ✅ `Application.runInBackground=true` (crítico)
- ✅ Handshake v24, login, reconciliação via `last_input_seq`

### Render
- ✅ Paper doll modular Mana Seed (body + outfit + hair via 3 SpriteRenderers)
- ✅ Walk cycle 6 frames @ 135ms, direção derivada de velocity (S/N/E/W)
- ✅ TileMapRenderer com summer_tileset (FLOOR/WALL/DIRT/WATER/DUNGEON_FLOOR)
- ✅ Entity tint verde para membros de party; flash branco + damage numbers ao receber hit
- ✅ CinemachineCamera + Pixel Perfect Camera, PPU=16 tiles / PPU=64 chars

### UI
- ✅ Login (user/pass, "lembrar conta" via PlayerPrefs)
- ✅ HUD (HP/MP/ST/XP), Chat (Enter/Esc, IsTyping bloqueia input)
- ✅ Inventário 24 slots + drag-drop (swap + stack-merge)
- ✅ 3 EquipSlots (Weapon/Armor/Ring) com Shift+Click e drag
- ✅ ShopUI (botão Loja/N), VaultUI (botão Baú/B), Personagem (C) com stats/pontos/proficiências
- ✅ Party UI (convite central + card de membros no canto)
- ✅ Downed UI (StandUp via ESPAÇO), Tooltip, Minimap, PingIndicator, DamageNumber
- ✅ Mensagens `[SYS]` dourado; Kick derruba pro Login

### Gotchas conhecidos
- ✅ Shift+Click como alternativa universal ao right-click (trackpad Mac)
- ✅ `EventSystem.pixelDragThreshold = 5` em `GameManager.Start()`
- ✅ InputHandler bloqueia Primary/Secondary quando cursor está sobre UI
- ✅ `InventoryUpdate` NÃO carrega equipment — vem via `StatsUpdate`

---

## Web (`crates/web`)

- ✅ HTTP server em `:8090` — endpoint `/api/register` cria conta com argon2id
- ⬜ Recuperação de senha
- ⬜ Painel de admin / moderação

---

## Persistência

- ✅ Postgres em `postgres://solar:solar_dev_123@localhost:5432/mmo_dev`
- ✅ Tabelas: accounts, characters (unspent_points, allocated_points[]), inventory, equipment, vault, proficiencies
- ⬜ **Migration: dropar `accounts.class`** (hoje persiste "none" pra novos registros; design é classless)
- ⬜ Backups / rotação no dev
- ⬜ Migrations versionadas (ex: `sqlx migrate` ou `refinery`)

---

## Cross-platform

Unity cobre os alvos de cliente; servidor Rust roda em Linux/macOS.

- ✅ macOS (editor Unity + servidor Rust nativo)
- ⬜ Build Unity Windows / Linux
- ⬜ Build Unity WebGL (WebSocket já compatível)
- ⬜ Build Unity Android / iOS (touch + lifecycle)
- ⬜ Servidor em Docker (Linux container)

---

## Infra / CI

- ⬜ GitHub Actions — `cargo check + test + clippy` no 2dEngine, build headless de Unity
- ⬜ `cargo deny` (licenças + CVEs)
- ⬜ Docker para o servidor
- ⬜ Observabilidade: tracing → OTLP → Grafana/Loki
- ⬜ Métricas de tick (p50/p99, entidades ativas, sessões)

---

## Roadmap de Fases

| Fase | Status | Goal |
|------|--------|------|
| **1 — Scaffold** | ✅ | Loop end-to-end: cliente conecta, servidor simula, snapshots |
| **2 — Mundo Jogável** | ✅ | Tiles, sprites Mana Seed paper doll, HUD, chat, nomes |
| **3 — Combate** | ✅ | Projéteis + melee cone, dano, Downed State, NPCs com IA, loot |
| **4 — Persistência** | ✅ | Postgres, argon2id, contas, inventário/equip/vault, stats alocados |
| **5 — Escala/Perf** | 🚧 | Spatial hash, delta snapshots, bincode, colisão server-side, rate-limit |
| **6 — Cross-platform** | ⬜ | Unity Windows/Linux/WebGL/mobile, servidor em Docker |
| **7 — Conteúdo** | ⬜ | Biomas adicionais, bosses, economia expandida, polish |
| **8 — Infra/Ops** | ⬜ | CI/CD, observabilidade, migrations versionadas, sharding |
