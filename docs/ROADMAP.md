# Roadmap

> Cada fase tem um **goal** testável. Não avançar antes do goal estar demonstrável.

---

## Fase 1 — Scaffold e loop end-to-end ✅

**Goal:** cliente conecta, vê a si mesmo e outros clientes como quadrados coloridos movendo-se sincronizados pelo servidor autoritativo.

- [x] Workspace Cargo (shared / server / web)
- [x] Protocolo JSON (Handshake, Login, Input, Snapshot, Chat)
- [x] Servidor tokio com tick loop 30Hz e AOI naive
- [x] Input WASD + envio de InputFrames
- [x] Cliente Unity scaffoldado (Universal 2D URP, Unity 6.4)
- [x] MCP for Unity configurado (HTTP :8080)
- [ ] `cargo check` verde no workspace
- [ ] Teste manual: servidor Rust + cliente Unity conectam, snapshot chega

---

## Fase 2 — Mundo jogável mínimo (2 semanas)

**Goal:** ilha navegável com tiles Mana Seed, sprite do jogador animado (paper doll), múltiplos jogadores visíveis, chat funcional.

### Unity (cliente)
- [ ] Importer de sprite sheets Mana Seed (64×64px, paper doll em layers)
- [ ] Tilemap 16×16 com Mana Seed Forest tileset
- [ ] Cena Login (TMP, NetClient, LoginUI)
- [ ] Cena Game (câmera Pixel Perfect 320×180, Cinemachine follow, Tilemap, 2D Global Light)
- [ ] EntityRenderer: recebe EntitySnapshot e renderiza sprite correto
- [ ] Interpolação de posição entre snapshots (buffer 2–3, renderiza 100ms no passado)
- [ ] HP bar + nome sobre entidade
- [ ] Chat UI (TMP + scroll)

### Rust (servidor)
- [ ] MapFile com tileset de ilha rasa (gerado proceduralmente ou hardcoded)
- [ ] Tiles bloqueantes (paredes, água). Colisão AABB vs grid
- [ ] Predição client-side + reconciliação (`last_input_seq`)
- [ ] Spatial hash grid (substitui AOI O(E×S))

---

## Fase 3 — Combate e NPCs (3 semanas)

**Goal:** atirar em mobs, tomar dano, cair em Downed State, ser revivido ou morrer.

- [ ] `Projectile` entity: `lifetime`, `speed`, `damage`, `owner`
- [ ] Ataque básico: cooldown por tipo de arma
- [ ] Colisão projétil vs entidade (broadphase spatial grid, narrow AABB)
- [ ] `Enemy { kind, ai_state }` com state machine idle → chase → attack
- [ ] **Downed State:** HP → 0 = Downed (não morte). Barra `dhp`, timer auto-revival
- [ ] Execução: player pode executar Downed inimigo (cast 3s)
- [ ] Loot: mob dropa entidade `Loot { item_id, qty }` ao morrer
- [ ] Coleta de loot por proximidade + INTERACT

---

## Fase 4 — Progressão Classless (3 semanas)

**Goal:** contas persistentes com proficiências funcionando. Equipar espada diferente de cajado em termos de dano.

- [ ] Postgres (sqlx): schema `accounts`, `characters`, `inventory`, `proficiencies`
- [ ] Auth: argon2id, sessão por token
- [ ] **Nível Principal** (1–100): XP geral → pontos de atributo (FOR/DES/INT/VIT/SPD)
- [ ] **Proficiências por uso:** tabela `proficiency_xp[kind]`, cap = f(level)
- [ ] Tipos de arma: Espada, Arco, Cajado. Dano modificado por proficiência + atributo
- [ ] Slots de equipamento: arma + armadura + anel×2
- [ ] Durabilidade de itens: reduz com uso e na morte
- [ ] Vault (baú permanente no Porto): depositar/retirar itens entre mortes
- [ ] Snapshot periódico do personagem → DB (30s + logout)

---

## Fase 5 — Navegação e Ilhas (3 semanas)

**Goal:** jogador navega de barco do Porto até ilha distante. Mapa do mundo visível.

- [ ] Entidade `Boat`: item equipável de transporte. Velocidade e HP próprios
- [ ] Mapa do mundo: grid de ilhas com tier e distância do Porto
- [ ] Gerador procedural de ilhas (BSP ou drunkard's walk) por tier
- [ ] Transição entre mapas (portal de barco → servidor troca MapFile)
- [ ] Barco pode ser atacado em zonas PvP (tier 2+)
- [ ] **Sistema de Aura/Poise:** tabela `aura_xp`, ganho em kills PvP, perda em mortes
- [ ] Mecânica de carregar Downed (`F` key, penalidade de movimento)

---

## Fase 6 — Cross-platform (4 semanas)

**Goal:** roda nativo em desktop (Win/Mac/Linux) e mobile (Android + iOS) com crossplay.

- [ ] Android: Input System touch, joystick virtual, lifecycle pause/resume
- [ ] iOS: Metal backend, TestFlight beta
- [ ] Windows/Linux: validar CI (GitHub Actions)
- [ ] Build scripts por plataforma

---

## Fase 7 — Conteúdo e lançamento (∞)

**Goal:** um jogo que vale a pena jogar.

- [ ] 15+ tipos de mobs com comportamentos distintos
- [ ] 3+ bosses com mecânicas específicas
- [ ] 5 tiers de raridade de loot funcionando
- [ ] Biomas completos: Ilhas Rasas, Vulcânicas, Amaldiçoadas
- [ ] Party-finding + guild system
- [ ] Trade P2P entre jogadores
- [ ] Leaderboard de Aura e Fama
- [ ] Múltiplos shards com matchmaking

---

## Infra (paralelo a tudo)

- [ ] CI: `cargo check`, `cargo test`, `cargo clippy` em PR
- [ ] Docker para o servidor Rust
- [ ] Observabilidade: `tracing` → OTLP → Grafana
- [ ] Rate-limit + DDoS mitigation no WS
- [ ] Backup automático do DB

---

## Decisões futuras

- [ ] Monetização: F2P cosméticos? B2P?
- [ ] Hospedagem: Hetzner VPS → Kubernetes conforme escala
- [ ] Mods: Lua/Rhai embarcado para scripts de enemy/item?
- [ ] Permadeath configurável: servidor em modo soft (mantém personagem) ou hard?
