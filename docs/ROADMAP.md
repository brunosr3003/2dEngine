# Roadmap

> Atualizado: 2026-04-24
> Cada fase tem um **goal** testável. Não avançar antes do goal estar demonstrável.
>
> As fases 1–4 estão entregues. As seguintes dividem-se em **Conteúdo** (navegação,
> PvP, bosses) e uma trilha paralela de **Escala/Infra** que pode ser puxada sob demanda.

---

## Fase 1 — Scaffold e loop end-to-end ✅

**Goal:** cliente conecta, vê a si mesmo e outros clientes no servidor autoritativo.

- [x] Workspace Cargo (shared / server / web). Crate `engine` existia e foi removido depois.
- [x] Protocolo JSON externally-tagged (Handshake, Login, Input, Snapshot, Chat)
- [x] Servidor tokio com tick loop 30Hz e AOI naive
- [x] Input WASD + envio de InputFrames
- [x] Cliente Unity (Universal 2D URP, Unity 6.4) — substituiu o cliente Rust
- [x] MCP for Unity configurado (HTTP :8080)
- [x] Servidor Rust + cliente Unity conectam; snapshots fluem

---

## Fase 2 — Mundo jogável mínimo ✅

**Goal:** ilha navegável com tiles Mana Seed, paper doll animado, múltiplos jogadores, chat.

### Unity (cliente)
- [x] Importer de sprite sheets Mana Seed (64×64 char, 16×16 tiles)
- [x] Tilemap com summer_tileset e tiles FLOOR/WALL/DIRT/WATER/DUNGEON_FLOOR
- [x] Cena Login (TMP, NetClient, LoginUI com "lembrar conta")
- [x] Cena Game (Cinemachine follow, Pixel Perfect Camera, 2D Global Light)
- [x] EntityRenderer: recebe EntitySnapshot e renderiza sprite correto
- [x] Interpolação de posição entre snapshots
- [x] HP bar + nome sobre entidade; damage numbers; flash branco no hit
- [x] Chat UI com `[SYS]` dourado, Kick message, LoginDenied reason
- [x] Paper doll modular (body + outfit + hair) com walk cycle 6 frames @ 135ms

### Rust (servidor)
- [x] MapFile + world gen procedural (rooms + corridors)
- [x] Tiles bloqueantes; colisão AABB vs grid via `move_and_slide`
- [x] Reconciliação via `last_input_seq`

---

## Fase 3 — Combate e NPCs ✅

**Goal:** atirar/golpear mobs, tomar dano, cair em Downed State, ser revivido ou morrer.

- [x] `Projectile` entity com lifetime/speed/damage/owner
- [x] Armas ranged (bow/wand/staff) = projéteis; melee (sword/great_sword/dagger/unarmed) = cone attack (`MELEE_RANGE=1.8`, `π/3` half-angle)
- [x] Cooldown por tipo de arma
- [x] Colisão projétil vs entidade
- [x] `Enemy { kind, ai_state }` com state machine wander → chase → attack
- [x] Múltiplos kinds (Grunt/Tank/Ninja/Berserker/Boss) com `defense` fixo
- [x] Cálculo: `dano_real = max(1, dmg_incoming - defense)`
- [x] Downed State (HP→0 vira Downed, timer auto-revival, StandUp via ESPAÇO)
- [x] Loot drop ao matar inimigo + coleta automática / via INTERACT
- [x] Dungeon room (24×20) no canto inferior-direito, acessível só via portais

---

## Fase 4 — Progressão Classless ✅

**Goal:** contas persistentes com proficiências funcionando. Arma diferente = dano diferente via scaling.

- [x] Postgres: `accounts`, `characters`, `inventory`, `equipment`, `vault`, `proficiencies`, `allocated_points[]`, `unspent_points`
- [x] Auth argon2id (`auth.rs`); re-hash on login quando necessário
- [x] Level principal com `POINTS_PER_LEVEL=3` pontos livres por level-up
- [x] Alocação via `AllocStatPoint { stat }` → `StatPointsUpdate`
- [x] Proficiências passivas escalando stats (`weapon_scaling × prof_level`)
- [x] 6 stats: hp_max, mp_max, atk, dex, wis, res (bonus em `STAT_POINT_BONUS`)
- [x] Slots de equipamento: Weapon, Armor, Ring (drag+Shift+Click)
- [x] Vault permanente no NPC(2) com 24 slots (deposit/withdraw)
- [x] Shop no NPC(1) com `ShopOpen/Buy/Close`
- [x] Snapshot periódico do personagem → DB
- [x] Drop de inv+equip no death
- [ ] **Migration: dropar `accounts.class`** (hoje persiste "none"; design é classless — pendente)

---

## Fase 5 — Navegação e Ilhas 🚧

**Goal:** jogador navega de barco do nexus até ilha distante. Mapa do mundo visível.

- [ ] Entidade `Boat`: item equipável de transporte, velocidade e HP próprios
- [ ] Mapa do mundo: grid de ilhas com tier e distância do nexus
- [ ] Gerador procedural por tier (BSP ou drunkard's walk)
- [ ] Transição entre mapas (server troca MapFile) — hoje só `MapChange` pro nexus
- [ ] Multi-map no `GameWorld` (TODO v2 já comentado em `world.rs:172`)
- [ ] Barco atacável em zonas PvP (tier 2+)
- [ ] Sistema Aura/Poise: `aura_xp`, ganho em kills PvP, perda em mortes
- [ ] Carregar Downed (`F` key, penalidade de movimento)

---

## Fase 6 — Conteúdo ⬜

**Goal:** um mundo que vale a pena explorar.

- [ ] 15+ tipos de mobs com comportamentos distintos (hoje ~5 kinds)
- [ ] 3+ bosses com mecânicas específicas
- [ ] 5 tiers de raridade de loot
- [ ] Biomas: Ilhas Rasas, Vulcânicas, Amaldiçoadas
- [ ] Quests / objetivos recorrentes
- [ ] NPCs adicionais (hoje só Npc(1) vendor, Npc(2) vault)

---

## Fase 7 — Social e Economia ⬜

**Goal:** jogadores cooperam e competem além do combate.

- [x] Party básica (convite `/party <nome>` ou PartyInvite, aceitar/recusar, sair, membros tintados de verde)
- [ ] Party-finding UI
- [ ] Guild system
- [ ] Trade P2P entre jogadores
- [ ] Leaderboard de Aura e Fama
- [ ] Chat global / guild / party channels

---

## Fase 8 — Cross-platform ⬜

**Goal:** roda em desktop e mobile com crossplay.

- [ ] Build Unity Windows / Linux
- [ ] Build Unity WebGL (WebSocket já compatível)
- [ ] Build Unity Android (touch, joystick virtual, lifecycle pause/resume)
- [ ] Build Unity iOS (TestFlight beta)
- [ ] Build scripts por plataforma + CI

---

## Trilha paralela — Escala / Perf / Infra

Pode ser puxada sob demanda. Itens por ordem de ROI técnico.

### Servidor
- [ ] **Spatial hash grid** para AOI (hoje O(E×S) naïve — não escala > ~100 entidades)
- [ ] Colisão server-side contra tilemap (cliente pode atravessar parede se mentir)
- [ ] Swept AABB para projéteis rápidos (anti-tunneling)
- [ ] Colisão entidade vs entidade (broadphase + narrow)
- [ ] Rate-limit por sessão (flood protection)
- [ ] Sharding: múltiplos mundos em processos separados
- [ ] Múltiplos mapas dentro do mesmo processo (ver TODO em `world.rs:172`)

### Protocolo / Banda
- [ ] Trocar JSON por **bincode** ou messagepack (maior ganho; ~3–5× menos bytes)
- [ ] Delta snapshots (só o que mudou desde o último ack)
- [ ] Quantização de posição (Vec2 → fixed-point 16-bit, ~50% banda)
- [ ] Compressão (zstd sobre o wire format)
- [ ] Eventos discretos (spawn/death/damage/loot fora do Snapshot)

### Infra / CI / Ops
- [ ] GitHub Actions: `cargo check + test + clippy` + build headless de Unity por PR
- [ ] `cargo deny` (licenças + CVEs)
- [ ] Docker para o servidor Rust
- [ ] Observabilidade: `tracing` → OTLP → Grafana/Loki
- [ ] Métricas de tick (p50/p99, entidades ativas, sessões)
- [ ] Migrations versionadas (sqlx migrate ou refinery)
- [ ] Backup automático do Postgres

---

## Decisões em aberto

- [ ] Monetização: F2P cosméticos? B2P?
- [ ] Hospedagem: Hetzner VPS → Kubernetes conforme escala
- [ ] Mods: Lua/Rhai embarcado para scripts de enemy/item?
- [ ] Permadeath configurável: servidor em modo soft (mantém personagem) ou hard?
- [ ] QUIC (quinn) pra clientes nativos + WebSocket fallback pro WebGL?
